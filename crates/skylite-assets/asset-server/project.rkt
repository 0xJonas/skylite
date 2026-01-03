#lang racket

(require file/glob)
(require racket/set)
(require "./log-trace.rkt")
(require "./project-assets.rkt")
(require "./nodes.rkt")
(require "./sequences.rkt")

(provide current-project retrieve-project list-assets asset-exists? retrieve-asset compute-asset-id
         (struct-out project)
         (struct-out asset)
         (struct-out tracked-file))

(struct tracked-file (path time hash))
(struct project (root-asset-file root-asset-name asset-files))
(struct asset (name type file tracked-paths thunk))


; (project-root asset-name) -> asset-data
(define asset-cache (make-immutable-hash))
; (project-root asset-name) -> asset-meta
(define open-assets (make-immutable-hash))
; project-root -> project
(define open-projects (make-immutable-hash))

(define current-project (make-parameter #f))


(define (raw->asset raw asset-file)
  (unless (pair? raw)
    (raise-asset-error "Invalid asset in ~a: expected pair [\"<name>\" . <definition>]" asset-file))
  (define name (car raw))
  (unless (symbol? name)
    (raise-asset-error "Asset name must be a symbol, got ~v" name))
  (define asset-def (cdr raw))

  (unless (list? asset-def) (raise-asset-error "Body must be an associative list"))

  (define type (cdr (or (assq 'type asset-def)
                        (raise-asset-error "Missing required field 'type"))))
  (unless (memq type '(project node node-list sequence)) (raise-asset-error "~v is not a valid type" type))

  (define get (cdr (or (assq 'get asset-def)
                       (raise-asset-error "Missing required field 'get"))))
  (unless (procedure? get) (raise-asset-error "'get must be a procedure"))

  (define base-path (path-only asset-file))
  (define tracked-paths-raw (cdr (or (assq 'tracked-paths asset-def) '(tracked-paths . ()))))
  (unless (list? tracked-paths-raw)
    (raise-asset-error "'tracked-paths must be a list of paths, got ~v" tracked-paths-raw))

  (define tracked-paths
    (for/list ([p tracked-paths-raw])
      (cond
        [(path? p) (build-path base-path p)]
        [(string? p) (build-path base-path (string->path p))]
        [else (raise-asset-error "'tracked-paths element not a string or path: ~v" p)])))

  (define tracked-paths-with-hash
    (for/list ([p tracked-paths]) (tracked-file p (current-seconds) (sha256-bytes (open-input-file p)))))

  (asset name type asset-file tracked-paths-with-hash get))


(define (load-assets-from-file path)
  (define raw-assets
    (let ([ns (make-base-namespace)])
      (parameterize ([current-namespace ns])
        (dynamic-require path 'skylite-assets))))

  (parameterize ([current-error-context
                  (struct-copy error-context (current-error-context)
                               [asset-file path])])
    (for/list ([raw-asset raw-assets])
      (raw->asset raw-asset path))))


(define (path->tracked-file path)
  (tracked-file path (current-seconds) (sha256-bytes (open-input-file path))))


(define (tracked-files-equal? a b)
  (and (equal? (tracked-file-path a) (tracked-file-path b))
       (equal? (tracked-file-hash a) (tracked-file-hash b))))


; Updates the given tracked-file by recalculating its hash, if the file may have changed.
(define (update-tracked-file tfile)
  (define file-timestamp (file-or-directory-modify-seconds (tracked-file-path tfile)))
  (if (<= (tracked-file-time tfile) file-timestamp)
      (struct-copy tracked-file tfile
                   [time (current-seconds)]
                   [hash (sha256-bytes (open-input-file (tracked-file-path tfile)))])
      (struct-copy tracked-file tfile [time (current-seconds)])))


; Finds a tracked-file corresponding to a given path in a list of tracked files.
(define (find-tracked-file path tfiles)
  (findf (lambda (tf) (equal? (tracked-file-path tf) path)) tfiles))


(define (load-project-root-asset project-root)
  (define assets (load-assets-from-file project-root))
  (define project-entry (or (findf (lambda (asset) (eq? (asset-type asset) 'project)) assets)
                            (raise-user-error "No 'project asset found in project root ~a" project-root)))
  (define root-asset-name (asset-name project-entry))
  (define project-asset-def (refine-project ((asset-thunk project-entry))))

  (set! asset-cache (hash-set asset-cache (cons project-root root-asset-name)
                              project-asset-def))
  (values project-asset-def root-asset-name assets))


(define (list-asset-files root glob-paths)
  (define base-path (path-only root))
  (set->list
   (set-remove
    (apply set-union
           (for/list ([glob-path glob-paths])
             (define glob-path-full (if (absolute-path? glob-path)
                                        glob-path
                                        (build-path base-path glob-path)))
             (apply set (glob glob-path-full))))
    root)))


(define (refresh-project-asset-files root glob-paths prev-asset-tfiles)
  (define new-asset-paths (list-asset-files root glob-paths))

  (define-values (unchanged-tfiles changed-tfiles)
    (for/fold ([unchanged '()] [changed '()])
              ([tfile prev-asset-tfiles]
               #:when (and (file-exists? (tracked-file-path tfile))
                           (member (tracked-file-path tfile) new-asset-paths)))
      (let ([updated (update-tracked-file tfile)])
        (if (tracked-files-equal? tfile updated)
            (values (cons updated unchanged) changed)
            (values unchanged (cons updated changed))))))

  (define new-tfiles
    (for/list ([path new-asset-paths]
               #:when (not (find-tracked-file path prev-asset-tfiles)))
      (path->tracked-file path)))

  (values unchanged-tfiles (append changed-tfiles new-tfiles)))


(define (refresh-project-assets! project-root unchanged-tfiles new-tfiles)
  (define (add-assets-from-tfile ht tfile)
    (for/fold ([ht ht]) ([asset (load-assets-from-file (tracked-file-path tfile))])
      (hash-set ht (cons project-root (asset-name asset)) asset)))

  (define (removed-asset? asset-key asset)
    (and (equal? (car asset-key) project-root)
         (not (find-tracked-file (asset-file asset) unchanged-tfiles))))

  (define removed-asset-keys (map car (filter (lambda (kv) (removed-asset? (car kv) (cdr kv))) (hash->list open-assets))))

  ; Remove changed and deleted assets
  (set! open-assets
        (for/fold ([open-assets open-assets]) ([key removed-asset-keys])
          (hash-remove open-assets key)))
  (set! asset-cache
        (for/fold ([asset-cache asset-cache]) ([key removed-asset-keys])
          (hash-remove asset-cache key)))

  ; Add changed and new assets
  (set! open-assets
        (for/fold ([open-assets open-assets]) ([tfile new-tfiles])
          (add-assets-from-tfile open-assets tfile))))


(define (load-or-update-project prev-project project-root)
  (if prev-project
      (log/trace 'info "Updating project ~a" project-root)
      (log/trace 'info "Opening project ~a" project-root))

  ; Handle the project root file separately from other asset files.
  (define prev-root-tfile (and prev-project (project-root-asset-file prev-project)))
  (define new-root-tfile
    (if (and prev-root-tfile (equal? (tracked-file-path prev-root-tfile) project-root))
        (update-tracked-file prev-root-tfile)
        (path->tracked-file project-root)))

  ; Project definition, name of the project's root asset, additional assets included in the root file.
  (define-values (project-asset-def root-asset-name additional-assets)
    (if (and prev-root-tfile (tracked-files-equal? prev-root-tfile new-root-tfile))
        (parameterize ([current-project prev-project])
          (values (let-values ([(_ def) (retrieve-asset 'project (project-root-asset-name prev-project))])
                    def)
                  (project-root-asset-name prev-project)
                  '()))
        (load-project-root-asset project-root)))

  (define prev-asset-tfiles (or (and prev-project (project-asset-files prev-project)) '()))
  (define-values (unchanged-tfiles new-tfiles)
    (refresh-project-asset-files project-root (project-asset-globs project-asset-def) prev-asset-tfiles))

  (refresh-project-assets! project-root unchanged-tfiles new-tfiles)

  (set! open-assets
        (for/fold ([open-assets open-assets]) ([asset additional-assets])
          (hash-set open-assets (cons project-root (asset-name asset)) asset)))

  (project new-root-tfile
           root-asset-name
           (append unchanged-tfiles new-tfiles)))


; Returns the project for the given project root.
; If the project is not known, this function will try to load it.
(define/trace (retrieve-project project-root)
  #:enter 'debug (format "Retrieving project ~a" project-root)
  #:exit 'debug (format "Finished loading project ~a" project-root)

  (parameterize ([current-error-context (error-context project-root #f #f)])
    (define prev-project (hash-ref open-projects project-root #f))

    (if (and prev-project
             (for/and ([tfile (cons (project-root-asset-file prev-project) (project-asset-files prev-project))])
               (tracked-files-equal? tfile (update-tracked-file tfile))))
        ; Fast path: no asset files changed
        prev-project

        ; Project was not open or some files have changed
        (let ([new-project (load-or-update-project prev-project project-root)])
          (set! open-projects (hash-set open-projects project-root new-project))
          new-project))))


(define (list-assets type)
  (define project-root (tracked-file-path (project-root-asset-file (current-project))))
  (define sorted-assets
    (sort
     (filter (lambda (entry)
               (and (equal? (caar entry) project-root)
                    (eq? (asset-type (cdr entry)) type)))
             (hash->list open-assets))
     symbol<?
     #:key cdar))
  (map cdr sorted-assets))


(define (asset-exists? req-type req-name)
  (define asset-key (cons (project-root-asset-file (current-project)) req-name))
  (define asset-inst (hash-ref open-assets asset-key #f))
  (and asset-inst (eq? (asset-type asset-inst) req-type)))


; Refining an asset performes multiple transformations to make
; further usage of the asset easier, such as:
; - Type checking and general validation.
; - Adding explicit type information to certain values, e.g. in node instances and sequences.
; - Renaming local backwards/forwards labels in sequences to regular labels.
; Assets are stored in their refined form in the asset cache.
(define (refine-asset type asset-data)
  (define (retrieve-node node-name)
    (let-values ([(_ data) (retrieve-asset 'node node-name)])
      data))

  (match type
    ['project (refine-project asset-data)]
    ['node (refine-node asset-data asset-exists?)]
    ['node-list (refine-node-list asset-data asset-exists? retrieve-node)]
    ['sequence (refine-sequence asset-data asset-exists? retrieve-node)]))


(define/trace (retrieve-asset req-type req-name)
  #:enter 'debug (format "Retrieving asset ~a from project ~a" req-name (tracked-file-path (project-root-asset-file (current-project))))

  (define project-root (tracked-file-path (project-root-asset-file (current-project))))
  (parameterize ([current-error-context (error-context project-root #f #f)])

    (define asset-key (cons project-root req-name))
    (define asset-inst (hash-ref open-assets asset-key
                                 (lambda () (raise-asset-error "Asset ~v not found in project ~a" req-name project-root))))
    (unless (eq? (asset-type asset-inst) req-type)
      (raise-asset-error "Asset ~v in project ~a does not have type ~v" req-name project-root req-type))

    (define cached-entry (hash-ref asset-cache asset-key #f))

    (define new-tracked-paths
      (for/list ([tfile (asset-tracked-paths asset-inst)]) (update-tracked-file tfile)))
    (define no-tracked-paths-changed
      (for/and ([new-tfile new-tracked-paths] [prev-tfile (asset-tracked-paths asset-inst)])
        (tracked-files-equal? new-tfile prev-tfile)))

    (define/trace (eval-asset)
      #:enter 'info (format "Evaluating asset ~a in project ~a" req-name project-root)
      ((asset-thunk asset-inst)))

    (if (and cached-entry no-tracked-paths-changed)
        ; Cache entry still valid
        (values asset-inst cached-entry)

        ; Cache entry does not exist or is no longer valid
        (parameterize ([current-error-context (struct-copy error-context (current-error-context)
                                                           [asset-file (asset-file asset-inst)]
                                                           [asset-name (asset-name asset-inst)])])
          (let* ([asset-data (eval-asset)]
                 [new-asset (struct-copy asset asset-inst [tracked-paths new-tracked-paths])]
                 [refined-data (refine-asset req-type asset-data)])
            (set! asset-cache (hash-set asset-cache asset-key refined-data))
            (set! open-assets (hash-set open-assets asset-key new-asset))
            (values asset-inst refined-data))))))


(define (compute-asset-id project-root asset-inst)
  (for/fold ([id 0]) ([entry (hash->list open-assets)])
    (if (and (equal? project-root (caar entry))
             (eq? (asset-type (cdr entry)) (asset-type asset-inst))
             (symbol<? (cdar entry) (asset-name asset-inst)))
        (+ 1 id)
        id)))


(module+ test
  (require raco/testing)

  (define base-dir (make-temporary-directory))

  (define (define-asset path name type content tracked-paths)
    (define file-content
      (format
       "#lang racket
        (provide skylite-assets)
        (define asset ~a)
        (define (log-eval msg)
          (call-with-output-file ~v
            (lambda (out) (displayln msg out))
            #:exists 'append))
        (log-eval \"~a-file\")
        (define skylite-assets
          `([,~v . ([type . ,~v]
                   [get . ,(lambda () (log-eval \"~a-asset\") asset)]
                   [tracked-paths . ,~v])]))"
       content (path->string (build-path base-dir "eval.log")) name name type name tracked-paths))
    (call-with-output-file path (lambda (out) (write-string file-content out)) #:exists 'replace))


  (define (setup-test-project)
    (define-asset (build-path base-dir "project.rkt")
      'project 'project "'([name . test] [assets . (\"./node-1.rkt\")])" '())
    (define-asset (build-path base-dir "node-1.rkt")
      'node-1 'node "'()" '())
    (define-asset (build-path base-dir "node-2.rkt")
      'node-2 'node "'()" '("./node-1.rkt"))
    (void))


  (define (check-eval-log! expected)
    (call-with-input-file (build-path base-dir "eval.log")
      (lambda (in)
        (let* ([actual (port->string in)]
               [ok (equal? actual expected)])
          (unless ok (println actual))
          (test-log! ok)))))


  (setup-test-project)
  (define project-root (build-path base-dir "project.rkt"))

  ; Loading a project for the first time should evaluate all files as well as the project root asset.
  (current-project (retrieve-project project-root))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\n")

  ; Loading a project again without intermediate changes should not evaluate anything.
  (current-project (retrieve-project project-root))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\n")

  ; Retrieving a node for the first time should evaluate its asset thunk.
  (let-values ([(_1 _2) (retrieve-asset 'node 'node-1)]) (void))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\n")

  ; Retrieving a node again without intermediate changes should not evaluate anything.
  (let-values ([(_1 _2) (retrieve-asset 'node 'node-1)]) (void))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\n")

  ; Changing an asset file should cause it to be evaluated again.
  (void (define-asset (build-path base-dir "node-1.rkt")
          'node-1 'node "(list)" '()))
  (current-project (retrieve-project project-root))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\nnode-1-file\n")

  ; The asset itself also has to be reevaluated.
  (let-values ([(_1 _2) (retrieve-asset 'node 'node-1)]) (void))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\nnode-1-file\nnode-1-asset\n")

  ; Changing the project root should cause it to be evaluated again, as well as any new asset files.
  (void (define-asset (build-path base-dir "project.rkt")
          'project 'project "'([name . test] [assets . (\"./node-2.rkt\")])" '()))
  (current-project (retrieve-project (build-path base-dir "project.rkt")))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\nnode-1-file\nnode-1-asset\nproject-file\nproject-asset\nnode-2-file\n")

  ; Changing a tracked path should reevaluate the affected asset.
  (let-values ([(_1 _2) (retrieve-asset 'node 'node-2)]) (void))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\nnode-1-file\nnode-1-asset\nproject-file\nproject-asset\nnode-2-file\nnode-2-asset\n")
  (void (define-asset (build-path base-dir "node-1.rkt")
          'node-1 'node "'()" '()))
  (let-values ([(_1 _2) (retrieve-asset 'node 'node-2)]) (void))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\nnode-1-file\nnode-1-asset\nproject-file\nproject-asset\nnode-2-file\nnode-2-asset\nnode-2-asset\n")

  ; Retrieving the asset again should not evaluate anything.
  (let-values ([(_1 _2) (retrieve-asset 'node 'node-2)]) (void))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\nnode-1-file\nnode-1-asset\nproject-file\nproject-asset\nnode-2-file\nnode-2-asset\nnode-2-asset\n"))

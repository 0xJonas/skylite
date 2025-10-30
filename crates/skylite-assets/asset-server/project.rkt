#lang racket

(require file/glob)
(require "./log-trace.rkt")

(provide retrieve-project list-assets asset-exists? retrieve-asset compute-asset-id
         (struct-out project)
         (struct-out asset))

(struct project (root-asset-file root-asset-name last-check-timestamp asset-files))
(struct asset (name type file tracked-paths thunk))

; (project asset-name) -> (file-hash asset-data)
(define asset-cache (make-immutable-hash))
(define open-assets (make-immutable-hash))
(define open-projects (make-immutable-hash))


(define (raw->asset raw asset-file)
  (unless (pair? raw)
    (raise-user-error "Invalid asset in ~a: expected pair [\"<name>\" . <definition>]" asset-file))
  (define name (car raw))
  (define asset-def (cdr raw))
  (define (invalid msg)
    (raise-user-error 'raw->asset "Invalid definition for asset ~v (~a): ~a" name asset-file msg))

  (unless (list? asset-def) (invalid "Body must be an associative list."))

  (define type (cdr (or (assq 'type asset-def)
                        (invalid "Missing required field 'type."))))
  (unless (memq type '(project node node-list sequence)) (invalid (format "~v is not a valid type." type)))

  (define get (cdr (or (assq 'get asset-def)
                       (invalid "Missing required field 'get."))))
  (unless (procedure? get) (invalid "'get must be a procedure."))

  (define tracked-paths-raw (cdr (or (assq 'tracked-paths asset-def) '(tracked-paths . ()))))
  (unless (and (list? tracked-paths-raw)
               (for/and ([p tracked-paths-raw]) (string? p)))
    (invalid "'tracked-paths must be a list of strings."))
  (define tracked-paths
    (map (lambda (p)
           (let ([tracked-path (build-path (path-only asset-file) p)])
             (cons tracked-path (sha256-bytes (open-input-file tracked-path)))))
         tracked-paths-raw))

  (asset name type asset-file tracked-paths get))


(define (load-assets-from-file path)
  (define raw-assets
    (let ([ns (make-base-namespace)])
      (parameterize ([current-namespace ns])
        (dynamic-require path 'skylite-assets))))

  (for/list ([raw-asset raw-assets])
    (raw->asset raw-asset path)))


(define (file-changed? path last-check-timestamp last-check-hash)
  (define file-timestamp (file-or-directory-modify-seconds path))
  (and
   (<= last-check-timestamp file-timestamp)
   (let ([current-hash (sha256-bytes (open-input-file path))])
     (and (not (equal? current-hash last-check-hash))
          current-hash))))


(define (list-asset-files root glob-paths)
  (flatten
   (for/list ([glob-path glob-paths])
     (define glob-path-full (if (absolute-path? glob-path)
                                glob-path
                                (build-path root glob-path)))
     (glob glob-path-full))))


(define (partition-changed-files files last-check-timestamp)
  (let lp ([files files]
           [changed '()]
           [unchanged '()])
    (if (pair? files)
        (let ([changed-hash (file-changed? (caar files) last-check-timestamp (cdar files))])
          (if changed-hash
              (lp (cdr files) (cons (cons (caar files) changed-hash) changed) unchanged)
              (lp (cdr files) changed (cons (car files) unchanged))))
        (values changed unchanged))))


(define (load-project-root-asset project-root project-root-hash)
  (define assets (load-assets-from-file project-root))
  (define project-entry (or (findf (lambda (asset) (eq? (asset-type asset) 'project)) assets)
                            (raise-user-error "No 'project asset found in project root ~a" project-root)))
  (define root-asset-name (asset-name project-entry))
  (define project-asset-def ((asset-thunk project-entry)))

  (set! asset-cache (hash-set asset-cache (cons project-root root-asset-name)
                              (cons project-root-hash project-asset-def)))
  (values project-asset-def root-asset-name assets))


(define (load-or-update-project prev-project project-root)
  (if prev-project
      (log/trace 'info "Updating project ~a" project-root)
      (log/trace 'info "Opening project ~a" project-root))

  ; If the project was already loaded, extract some information for later comparisons.
  (define prev-asset-files (or (and prev-project (project-asset-files prev-project)) '()))
  (define prev-last-check-timestamp (or (and prev-project (project-last-check-timestamp prev-project)) -1))

  (define new-check-timestamp (current-seconds))

  (define-values (project-root-changed project-root-hash)
    (let* ([prev-file-and-hash (assoc project-root prev-asset-files)]
           [new-hash-if-changed (or (and (not prev-file-and-hash)
                                         (sha256-bytes (open-input-file project-root)))
                                    (file-changed? (car prev-file-and-hash)
                                                   prev-last-check-timestamp
                                                   (cdr prev-file-and-hash)))])
      (values new-hash-if-changed (or new-hash-if-changed (cdr prev-file-and-hash)))))

  ; Project definition, name of the project's root asset, additional assets included in the root file.
  (define-values (project-asset-def root-asset-name additional-assets)
    (if (not project-root-changed)
        (values (let-values ([(_ def) (retrieve-asset prev-project 'project (project-root-asset-name prev-project))])
                  def)
                (project-root-asset-name prev-project)
                '())
        (load-project-root-asset project-root project-root-hash)))

  (define-values (changed-asset-files unchanged-asset-files)
    (let ([asset-file-globs (cdr (or (assq 'assets project-asset-def)
                                     '(assets . ("./**/*.rkt"))))]
          [cons-prev-hash (lambda (file)
                            (cons file (cdr (or (assoc file prev-asset-files) '("" . -1)))))])
      (partition-changed-files
       (cons (cons project-root project-root-hash)
             (map cons-prev-hash (list-asset-files (path-only project-root) asset-file-globs)))
       prev-last-check-timestamp)))

  (set! open-assets
        (let* (; Remove changed or deleted assets
               [filtered
                (for/fold ([assets open-assets]) ([entry (hash->list open-assets)])
                  (if (or (not (equal? (caar entry) project-root))
                          (assoc (asset-file (cdr entry)) unchanged-asset-files))
                      assets
                      (hash-remove assets (car entry))))]

               ; Helper proc
               [add-assets (lambda (ht assets)
                             (for/fold ([ht ht]) ([a assets])
                               (hash-set ht (cons project-root (asset-name a)) a)))]

               ; Add changed or new assets
               [with-new-assets
                   (for/fold ([assets filtered]) ([file-and-hash changed-asset-files])
                     (add-assets assets (load-assets-from-file (car file-and-hash))))]

               ; Add assets from project root file
               [with-additional-assets (add-assets with-new-assets additional-assets)])
          with-additional-assets))

  (project project-root
           root-asset-name
           new-check-timestamp
           (append changed-asset-files unchanged-asset-files)))


; Returns the project for the given project root.
; If the project is not known, this function will try to load it.
(define/trace (retrieve-project project-root)
  #:enter 'debug (format "Retrieving project ~a" project-root)
  #:exit 'debug (format "Finished loading project ~a" project-root)

  (define prev-project (hash-ref open-projects project-root #f))

  (if (and prev-project
           (for/and ([asset-file (project-asset-files prev-project)])
             (not (file-changed? (car asset-file) (project-last-check-timestamp prev-project) (cdr asset-file)))))
      ; Fast path: no asset files changed
      prev-project

      ; Project was not open or some files have changed
      (let ([new-project (load-or-update-project prev-project project-root)])
        (set! open-projects (hash-set open-projects project-root new-project))
        new-project)))


(define (list-assets project)
  (filter (lambda (k) (equal? (car k) (project-root-asset-file project))) (hash-keys open-assets)))


(define (asset-exists? project-inst req-type req-name)
  (define asset-key (cons (project-root-asset-file project-inst) req-name))
  (define asset-inst (hash-ref open-assets asset-key #f))
  (and asset-inst (eq? (asset-type asset-inst) req-type)))


(define/trace (retrieve-asset project-inst req-type req-name)
  #:enter 'debug (format "Retrieving asset ~a from project ~a" req-name (project-root-asset-file project-inst))

  (define asset-key (cons (project-root-asset-file project-inst) req-name))
  (define asset-inst (hash-ref open-assets asset-key
                               (lambda () (raise-user-error 'retrieve-asset "Asset ~v not found in project ~a" req-name (project-root-asset-file project-inst)))))
  (unless (eq? (asset-type asset-inst) req-type)
    (raise-user-error 'retrieve-asset "Asset ~v in project ~a does not have type ~a" req-name (project-root-asset-file project-inst) req-type))

  (define asset-file-hash (cdr (assoc (asset-file asset-inst) (project-asset-files project-inst))))
  (define cached-entry (hash-ref asset-cache asset-key #f))

  (define tracked-paths-changed-hash
    (for/list ([tp (asset-tracked-paths asset-inst)])
      (file-changed? (car tp) (project-last-check-timestamp project-inst) (cdr tp))))
  (define new-tracked-paths
    (for/list ([tp (asset-tracked-paths asset-inst)]
               [ch tracked-paths-changed-hash])
      (if ch (cons (car tp) ch) tp)))

  (define/trace (eval-asset)
    #:enter 'info (format "Evaluating asset ~a in project ~a" req-name (project-root-asset-file project-inst))
    ((asset-thunk asset-inst)))

  (if (and cached-entry
           (equal? asset-file-hash (car cached-entry))
           (not (for/or ([t tracked-paths-changed-hash]) t)))
      ; Cache entry still valid
      (values asset-inst (cdr cached-entry))

      ; Cache entry does not exist or is no longer valid
      (let ([asset-data (eval-asset)]
            [new-asset (struct-copy asset asset-inst [tracked-paths new-tracked-paths])])
        (set! asset-cache (hash-set asset-cache asset-key (cons asset-file-hash asset-data)))
        (set! open-assets (hash-set open-assets asset-key new-asset))
        (values asset-inst asset-data))))


(define (compute-asset-id project-root asset-name)
  (for/fold ([id 0]) ([a (hash-keys open-assets)])
    (if (and (equal? project-root (car a)) (string<? (cdr a) asset-name))
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
          `([~v . ([type . ,~v]
                   [get . ,(lambda () (log-eval \"~a-asset\") asset)]
                   [tracked-paths . ,~v])]))"
       content (path->string (build-path base-dir "eval.log")) name name type name tracked-paths))
    (call-with-output-file path (lambda (out) (write-string file-content out)) #:exists 'replace))


  (define (setup-test-project)
    (define-asset (build-path base-dir "project.rkt")
      "project" 'project "'([name . test] [assets . (\"./node-1.rkt\")])" '())
    (define-asset (build-path base-dir "node-1.rkt")
      "node-1" 'node "'()" '())
    (define-asset (build-path base-dir "node-2.rkt")
      "node-2" 'node "'()" '("./node-1.rkt"))
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
  (define project (retrieve-project project-root))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\n")

  ; Loading a project again without intermediate changes should not evaluate anything.
  (set! project (retrieve-project project-root))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\n")

  ; Retrieving a node for the first time should evaluate its asset thunk.
  (let-values ([(_1 _2) (retrieve-asset project 'node "node-1")]) (void))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\n")

  ; Retrieving a node again without intermediate changes should not evaluate anything.
  (let-values ([(_1 _2) (retrieve-asset project 'node "node-1")]) (void))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\n")

  ; Changing an asset file should cause it to be evaluated again.
  (void (define-asset (build-path base-dir "node-1.rkt")
          "node-1" 'node "(list)" '()))
  (set! project (retrieve-project project-root))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\nnode-1-file\n")

  ; The asset itself also has to be reevaluated.
  (let-values ([(_1 _2) (retrieve-asset project 'node "node-1")]) (void))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\nnode-1-file\nnode-1-asset\n")

  ; Changing the project root should cause it to be evaluated again, as well as any new asset files.
  (void (define-asset (build-path base-dir "project.rkt")
          "project" 'project "'([name . test] [assets . (\"./node-2.rkt\")])" '()))
  (set! project (retrieve-project (build-path base-dir "project.rkt")))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\nnode-1-file\nnode-1-asset\nproject-file\nproject-asset\nnode-2-file\n")

  ; Changing a tracked path should reevaluate the affected asset.
  (let-values ([(_1 _2) (retrieve-asset project 'node "node-2")]) (void))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\nnode-1-file\nnode-1-asset\nproject-file\nproject-asset\nnode-2-file\nnode-2-asset\n")
  (void (define-asset (build-path base-dir "node-1.rkt")
          "node-1" 'node "'()" '()))
  (let-values ([(_1 _2) (retrieve-asset project 'node "node-2")]) (void))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\nnode-1-file\nnode-1-asset\nproject-file\nproject-asset\nnode-2-file\nnode-2-asset\nnode-2-asset\n")

  ; Retrieving the asset again should not evaluate anything.
  (let-values ([(_1 _2) (retrieve-asset project 'node "node-2")]) (void))
  (check-eval-log! "project-file\nproject-asset\nnode-1-file\nnode-1-asset\nnode-1-file\nnode-1-asset\nproject-file\nproject-asset\nnode-2-file\nnode-2-asset\nnode-2-asset\n"))

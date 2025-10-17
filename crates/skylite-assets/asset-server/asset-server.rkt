#lang racket

(require file/glob)
(require racket/unix-socket)
(require racket/date)
(require (for-syntax syntax/parse))

(define tracing-stack (make-parameter '()))

(struct log-metadata (timestamp trace))

(define (make-log-metadata)
  (log-metadata (current-date) (tracing-stack)))

(define-syntax (define/trace stx)
  (syntax-parse stx
    [(_ (name args ...)
        (~alt (~optional (~seq #:enter enter-level enter-msg))
              (~optional (~seq #:exit exit-level exit-msg)))
        ...
        body ...)
     #`(define (name args ...)
         (define (inner)
           body ...)

         (parameterize ([tracing-stack (cons (symbol->string 'name) (tracing-stack))])
           (~? (log-message (current-logger) enter-level enter-msg (make-log-metadata) #f) #f)

           (let ([$result (inner)])
             (~? (log-message (current-logger) exit-level exit-msg (make-log-metadata) #f) #f)
             $result)))]))

(define-syntax (log/trace stx)
  (syntax-parse stx
    [(_ level fmt fmt-args ...)
     #'(log-message (current-logger) level (format fmt fmt-args ...) (make-log-metadata) #f)]))

(struct project (assets))

(define asset-cache (make-hash))
(define open-projects (list))

(define (load-asset-file path)
  (let ([ns (make-base-namespace)])
    (parameterize ([current-namespace ns])
      (dynamic-require path 'skylite-assets))))

(define (list-asset-files root glob-paths)
  (flatten
   (for/list ([glob-path glob-paths])
     (define glob-path-full (if (absolute-path? glob-path)
                                glob-path
                                (build-path root glob-path)))
     (glob glob-path-full))))

(define/trace (load-project project-root)
  #:enter 'info (format "Loading project ~a" project-root)
  #:exit 'debug (format "Finished loading project ~a" project-root)

  ; Load project definition
  (define project-root-assets (load-asset-file project-root))
  (define project-asset-def
    (let ([res (findf (lambda (asset) (eq? (cadr asset) 'project)) project-root-assets)])
      (if res
          res
          (raise-user-error "Asset file ~a does not contain a 'project asset." project-root))))
  (define project-asset
    (let ([asset-name (car project-asset-def)]
          [asset-thunk (cddr project-asset-def)])
      (hash-ref! asset-cache (cons project-root asset-name) asset-thunk)))

  ; Load assets for project
  (define asset-file-globs (cdr (or (assq 'assets project-asset)
                                    '(assets . ("./**/*.rkt")))))
  (define asset-files (list-asset-files (path-only project-root) asset-file-globs))
  (define assets-defs (apply append (for/list ([file asset-files]) (load-asset-file file))))
  (define assets (for/fold ([assets (make-immutable-hash project-root-assets)]) ([new-asset assets-defs])
                   (hash-set assets (car new-asset) (cdr new-asset))))

  (project assets))

; Returns the project for the given project root.
; If the project is not known, this function will try to load it.
(define (retrieve-project project-root)
  (define res (assoc project-root open-projects))
  (if res
      (begin
        (log/trace 'debug "Project already loaded: ~a" project-root)
        (cdr res))
      (let ([project (load-project project-root)])
        (set! open-projects (cons (cons project-root project) open-projects))
        project)))

(define (retrieve-asset project-root assets asset)
  (define asset-def (hash-ref assets asset))
  (define asset-type (car asset-def))
  (define asset-thunk (cdr asset-def))
  (define/trace (eval-asset)
    #:enter 'info (format "Evaluating asset ~a in project ~a" asset project-root)
    (asset-thunk))

  (cons asset-type (hash-ref! asset-cache (cons project-root asset) eval-asset)))

(struct request-header (type project-root))
(struct asset-request-params (asset))

; Request Header:
; - request type: 1 Byte
; - project-root length: 2 Byte
; - project root string
(define (read-request-header in)
  (let ([req-type (read-byte in)])
    (if (eof-object? req-type)
        #f
        (let* ([project-root-len (integer-bytes->integer (read-bytes 2 in) #f)]
               [project-root (bytes->path (read-bytes project-root-len in))])
          (request-header req-type project-root)))))

(define (read-request-params request in)
  (match (request-header-type request)
    [0
     (define asset-len (integer-bytes->integer (read-bytes 2 in) #f))
     (define asset (bytes->string/utf-8 (read-bytes asset-len in) #f))
     (asset-request-params asset)]
    [1 '()]))

(define (process-request header params out)
  (match (request-header-type header)
    [0
     (define project-root (request-header-project-root header))
     (define project (retrieve-project project-root))
     (retrieve-asset project-root (project-assets project) (asset-request-params-asset params))
     ; TODO: Serialize
     ]
    [1
     (define project (retrieve-project (request-header-project-root header)))
     (hash-keys (project-assets project))
     ; TODO: Serialize
     ]))

(define (server-thread in out)
  (log/trace 'info "New connection")

  (let lp ([header (read-request-header in)])
    (define params (read-request-params header in))
    (process-request header params out)
    (let ([next-header (read-request-header in)])
      (when next-header (lp next-header)))))

(define (start-log-thread)
  (define receiver (make-log-receiver (current-logger) 'info))

  (define (format-log-msg msg)
    (match-let ([(vector level message data topic) msg])
      (match topic
        ['asset-server
         (format "~a [~a] ~a ~a: ~a"
                 (date->string (log-metadata-timestamp data) #t) topic (log-metadata-trace data)
                 level message)]
        [_ (format "xxxx-xx-xxTxx:xx:xx [~a] ~a: ~a" topic level message)])))

  (define log-ready (make-semaphore))

  (define (log-thread)
    (date-display-format 'iso-8601)
    (with-handlers ([exn:break? (void)]
                    [exn:break:hang-up? (void)]
                    [exn:break:terminate? (void)])
      (call-with-output-file "./asset-server.log"
        #:exists 'append
        (lambda (out)
          (semaphore-post log-ready)
          (let lp ()
            (displayln (format-log-msg (sync receiver)) out)
            (flush-output out)
            (lp))))))

  (void (thread log-thread))
  (semaphore-wait log-ready))

(module* main #f
  (define debug-mode
    (and (< 0 (vector-length (current-command-line-arguments)))
         (equal? (vector-ref (current-command-line-arguments) 0) "--debug")))

  (define io-addr "./socket")
  (define listener (unix-socket-listen io-addr))

  (current-logger (make-logger 'asset-server))

  (start-log-thread)

  (with-handlers ([exn:break? void]
                  [exn:break:hang-up? void]
                  [exn:break:terminate? void])
    (unless debug-mode
      (close-input-port (current-input-port))
      (close-output-port (current-output-port))
      (close-output-port (current-error-port)))

    (log/trace 'info "asset-server started")
    (let lp ([conn-idx 0])
      (let-values ([(in out) (unix-socket-accept listener)])
        (parameterize ([tracing-stack (list (format "conn-~a" conn-idx))])
          (thread (lambda () (server-thread in out))))
        (lp (+ 1 conn-idx)))))

  ; Cleanup
  (unix-socket-close-listener listener)
  (delete-file io-addr))

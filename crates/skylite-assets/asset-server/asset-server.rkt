#lang racket

(require racket/unix-socket)
(require "./log-trace.rkt")
(require "./project.rkt")
(require "./serde.rkt")

(struct request-header (type project-root))
(struct asset-request-params (asset-type asset-name))
(struct list-assets-request-params (asset-type))


(define (read-request-header in)
  (let ([req-type (read-byte in)])
    (if (eof-object? req-type)
        #f
        (request-header req-type
                        (bytes->path
                         (list->bytes
                          (vector->list
                           (deserialize-obj in '(vec . u8)))))))))


(define (asset-type->id type)
  (match type
    ['project 0]
    ['node 1]
    ['node-list 2]
    ['sequence 3]))


(define (id->asset-type v)
  (case v
    [(0) 'project]
    [(1) 'node]
    [(2) 'node-list]
    [(3) 'sequence]))


(define (read-request-params request in)
  (case (request-header-type request)
    [(0)
     (define asset-type (id->asset-type (deserialize-obj in 'u8)))
     (define asset-name (string->symbol (deserialize-obj in 'string)))
     (asset-request-params asset-type asset-name)]
    [(1)
     (define asset-type (id->asset-type (deserialize-obj in 'u8)))
     (list-assets-request-params asset-type)]))


(define (serialize-asset-meta out id asset)
  (serialize-obj out 'u32 id)
  (serialize-obj out 'string (symbol->string (asset-name asset)))
  (serialize-obj out 'u8 (asset-type->id (asset-type asset)))

  (define tracked-paths
    (vector-append
     (for/vector ([tfile (asset-tracked-paths asset)])
       (list->vector (bytes->list (path->bytes (tracked-file-path tfile)))))
     (vector (list->vector (bytes->list (path->bytes (asset-file asset)))))))
  (serialize-obj out '(vec . (vec . u8)) tracked-paths))


(define (error-response out exn)
  (serialize-obj out 'u8 1) ; Result err
  (let ([context (exn:asset-context exn)])
    (serialize-obj out 'string (if (error-context-project-root context)
                                   (path->string (error-context-project-root context))
                                   ""))
    (serialize-obj out 'string (if (error-context-asset-file context)
                                   (path->string (error-context-asset-file context))
                                   ""))
    (serialize-obj out 'string (if (error-context-asset-name context)
                                   (symbol->string (error-context-asset-name context))
                                   "")))
  (serialize-obj out 'string (exn:asset-message exn)))


(define (process-request header params out)
  (case (request-header-type header)
    [(0)
     (define project-root (request-header-project-root header))
     (with-handlers ([exn:asset? (lambda (exn) (error-response out exn))])
       (parameterize ([current-project (retrieve-project project-root)])
         (define-values (asset asset-data) (retrieve-asset (asset-request-params-asset-type params)
                                                           (asset-request-params-asset-name params)))
         (define asset-id (compute-asset-id (asset-type asset) (asset-name asset)))
         (serialize-obj out 'u8 0) ; Result ok
         (serialize-asset-meta out asset-id asset)
         (match (asset-type asset)
           ['project (serialize-project-asset out asset-data)]
           ['node (serialize-node out asset-data)]
           ['node-list (serialize-node-list out asset-data)]
           ['sequence (serialize-sequence out asset-data)])))
     (flush-output out)]
    [(1)
     (define project-root (request-header-project-root header))
     (with-handlers ([exn:asset? (lambda (exn) (error-response out exn))])
       (parameterize ([current-project (retrieve-project project-root)])
         (define assets (list-assets (list-assets-request-params-asset-type params)))
         (define num-assets (length assets))
         (serialize-obj out 'u8 0) ; Result ok
         (serialize-obj out 'u32 num-assets)
         (for ([asset assets] [id (build-list num-assets values)])
           (serialize-asset-meta out id asset))))
     (flush-output out)]))


(define (server-thread in out)
  (log/trace 'info "New connection")

  (let lp ([header (read-request-header in)])
    (define params (read-request-params header in))
    (process-request header params out)
    (let ([next-header (read-request-header in)])
      (when next-header (lp next-header)))))


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

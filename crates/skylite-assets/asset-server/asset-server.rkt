#lang racket

(require racket/unix-socket)
(require "./log-trace.rkt")
(require "./project.rkt")
(require "./base-serde.rkt")
(require "./nodes.rkt")

(struct request-header (type project-root))
(struct asset-request-params (asset-type asset-name))


; Request Header:
; - request type: 1 Byte
; - project-root length: 2 Byte
; - project root string
(define (read-request-header in)
  (let ([req-type (read-byte in)])
    (if (eof-object? req-type)
        #f
        (request-header req-type (string->path (deserialize-obj in 'string))))))


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
     (define asset-name (deserialize-obj in 'string))
     (asset-request-params asset-type asset-name)]
    [(1) '()]))


(define (serialize-asset-meta out id asset)
  (serialize-obj out 'u32 id)
  (serialize-obj out 'string (asset-name asset))
  (serialize-obj out 'u8 (asset-type->id (asset-type asset)))

  (define tracked-paths
    (vector-append
     (for/vector ([tp (asset-tracked-paths asset)])
       (path->string (car tp)))
     (vector (path->string (asset-file asset)))))
  (serialize-obj out '(vec . string) tracked-paths))


(define (process-request header params out)
  (case (request-header-type header)
    [(0)
     (define project-root (request-header-project-root header))
     (define project (retrieve-project project-root))
     (define-values (asset asset-data) (retrieve-asset project
                                                       (asset-request-params-asset-type params)
                                                       (asset-request-params-asset-name params)))
     (define asset-id (compute-asset-id project-root (asset-name asset)))
     (define out-bytes
       (match (asset-type asset)
         ['node (node->bytes out asset-data)]))
     (serialize-obj out 'u8 0) ; Result ok
     (serialize-asset-meta out asset-id asset)
     (write-bytes out-bytes out)
     (flush-output out)]
    [(1)
     (define project (retrieve-project (request-header-project-root header)))
     (list-assets project)
     ; TODO: Serialize
     ]))


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

#lang racket

(require racket/unix-socket)
(require "./log-trace.rkt")
(require "./project.rkt")

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
     (retrieve-asset project-root project (asset-request-params-asset params))
     ; TODO: Serialize
     ]
    [1
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

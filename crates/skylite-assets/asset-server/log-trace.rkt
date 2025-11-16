#lang racket

(require racket/date)
(require (for-syntax syntax/parse))

(provide define/trace log/trace start-log-thread tracing-stack
         (struct-out exn:asset) raise-asset-error
         (struct-out error-context) current-error-context)

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

           (let ([$result (call-with-values inner list)])
             (~? (log-message (current-logger) exit-level exit-msg (make-log-metadata) #f) #f)
             (apply values $result))))]))


(define-syntax (log/trace stx)
  (syntax-parse stx
    [(_ level fmt fmt-args ...)
     #'(log-message (current-logger) level (format fmt fmt-args ...) (make-log-metadata) #f)]))


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


(struct error-context (project-root asset-file asset-name))
(define current-error-context (make-parameter (error-context #f #f #f)))

(struct exn:asset (context tracing-stack message))


(define (raise-asset-error msg . params)
  (raise
   (exn:asset (current-error-context)
              (tracing-stack)
              (apply format (cons msg params)))))

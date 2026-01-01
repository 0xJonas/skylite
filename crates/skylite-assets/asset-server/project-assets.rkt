#lang racket

(require "./log-trace.rkt")
(require "./types.rkt")
(provide refine-project
         (struct-out project-asset))

(define (refine-project asset-data)
  (unless (list? asset-data)
    (raise-asset-error "'project asset must be a list, got ~v" asset-data))

  (define name
    (let ([p (assq 'name asset-data)])
      (or p (raise-asset-error "Missing required key 'name"))
      (unless (symbol? (cdr p)) (raise-asset-error "Value for 'name must be a symbol, got ~v" (cdr p)))
      (symbol->string (cdr p))))

  (define globs
    (let ([assets (cdr (or (assq 'assets asset-data) '(assets . ("./**/*.rkt"))))])
      (unless (list? assets) (raise-asset-error "Value for 'assets must be a list of globs, got ~v" assets))
      (unless (for/and ([g assets]) (string? g))
        (raise-asset-error "Value for 'assets must be a list of globs, got ~v" assets))
      assets))

  (project-asset name globs))

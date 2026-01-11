#lang racket

(require "./log-trace.rkt")
(require "./types.rkt")
(provide refine-project1 refine-project2
         (struct-out project-asset))

; project assets have to be refined in multiple steps, because
; during the first step, the other assets are not yet loaded, so
; the root node cannot be validated.

(define (refine-project1 asset-data)
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

  (define root-node
    (let ([r (assq 'root-node asset-data)])
      (unless r (raise-asset-error "Missing require key 'root-node"))
      (cdr r)))

  (project-asset name globs root-node))


(define (refine-project2 project asset-exists? retrieve-node)
  (struct-copy project-asset project
               [root-node (refine-value '(node . *) (project-asset-root-node project) asset-exists? retrieve-node)]))

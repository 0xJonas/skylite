#lang racket

(require "./project.rkt")
(require "./base-serde.rkt")

(provide node->bytes)


(define (serialize-variable out project var)
  (match-let ([(list name type) var])
    (unless (symbol? name) (raise-user-error 'serialize-node "Node name must be a symbol, got ~s" name))
    (unless (valid-base-type? type) (raise-user-error 'serialize-node "Not a valid type: ~s" type))
    (when (and (pair? type) (eq? (car type) 'node) (not (asset-exists? project 'node (symbol->string (cdr type)))))
      (raise-user-error 'serialize-node "Node ~a does not exist in project" (cdr type)))
    (serialize-obj out 'string (symbol->string name))
    (serialize-type out type)))


(define (node->bytes project asset-data)
  (unless (list? asset-data)
    (raise-user-error 'serialize-node "Node asset must be an alist"))

  (define out (open-output-bytes))

  (define parameters (assq 'parameters asset-data))
  (when parameters
    (let ([parameters-value (cdr parameters)])
      (if (list? parameters-value)
          (begin
            (serialize-obj out 'u32 (length parameters-value))
            (for ([var parameters-value]) (serialize-variable out project var)))
          (raise-user-error 'serialize-node "'parameters key must be a list of variables"))))

  (define properties (assq 'properties asset-data))
  (when properties
    (let ([properties-value (cdr properties)])
      (if (list? properties-value)
          (begin
            (serialize-obj out 'u32 (length properties-value))
            (for ([var properties-value]) (serialize-variable out project var)))
          (raise-user-error 'serialize-node "'properties key must be a list of variables"))))

  (get-output-bytes out))

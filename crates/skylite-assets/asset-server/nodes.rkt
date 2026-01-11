#lang racket

(require "./log-trace.rkt")
(require "./types.rkt")
(provide refine-node refine-node-list)


(define (refine-node asset-data)
  (define (validate-variable var)
    (unless (and (list? var) (= (length var) 2))
      (raise-asset-error "Variable must be a list with name and type, got ~v" var))
    (match-let ([(list name type) var])
      (unless (symbol? name)
        (raise-asset-error "Variable name must be a symbol, got ~v" name))
      (validate-type type)))

  (unless (list? asset-data)
    (raise-asset-error "Node asset must be an alist, got ~v" asset-data))

  (define parameters (cdr (or (assq 'parameters asset-data) '(parameters . ()))))
  (unless (list? parameters)
    (raise-asset-error "'parameters key must be a list of variables, got ~v" parameters))
  (for ([p parameters]) (validate-variable p))

  (define properties (cdr (or (assq 'properties asset-data) '(properties . ()))))
  (unless (list? properties)
    (raise-asset-error "'parameters key must be a list of variables, got ~v" properties))
  (for ([p properties]) (validate-variable p))

  (node parameters properties))


(define (refine-node-list asset-data compute-id retrieve-node)
  (unless (list? asset-data)
    (raise-asset-error "Node list must be a list, got ~v" asset-data))
  (for/list ([inst asset-data])
    (let* ([name (car inst)]
           [id (compute-id 'node name)]
           [refined (refine-value '(node . *) inst compute-id retrieve-node)])
      (cons (cons name id) (cdr refined)))))

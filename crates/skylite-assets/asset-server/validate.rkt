#lang racket

(require "./log-trace.rkt")
(provide validate-node validate-node-list)


(define (validate-type type asset-exists?)
  (match type
    [(or 'u8 'u16 'u32 'u64 'i8 'i16 'i32 'i64 'f32 'f64 'bool 'string 'project 'node-list 'sequence) (void)]
    [(cons 'vec item-type) (validate-type item-type asset-exists?)]
    [(cons 'node node)
     (unless (symbol? node)
       (raise-asset-error "Node must be a symbol, got ~a" node))
     (unless (or (eq? node '*) (asset-exists? 'node node))
       (raise-asset-error "Node ~a does not exist" node))]
    [(list item-types ...) (for ([item-type item-types]) (validate-type item-type asset-exists?))]
    [else (raise-asset-error "Unknown type ~a" else)]))


(define (check-type value type asset-exists? retrieve-node)
  (validate-type type asset-exists?)

  (match type
    ['u8 (unless (exact-integer? value)
           (raise-asset-error "Expected integer for 'u8, got ~v" value))
         (unless (<= 0 value #xff)
           (raise-asset-error "Value for u8 is out of range: ~a" value))]
    ['u16 (unless (exact-integer? value)
            (raise-asset-error "Expected integer for 'u16, got ~v" value))
          (unless (<= 0 value #xffff)
            (raise-asset-error "Value for u16 is out of range: ~a" value))]
    ['u32 (unless (exact-integer? value)
            (raise-asset-error "Expected integer for 'u32, got ~v" value))
          (unless (<= 0 value #xffffffff)
            (raise-asset-error "Value for u32 is out of range: ~a" value))]
    ['u64 (unless (exact-integer? value)
            (raise-asset-error "Expected integer for 'u64, got ~v" value))
          (unless (<= 0 value #xffffffffffffffff)
            (raise-asset-error "Value for u64 is out of range: ~a" value))]

    ['u8 (unless (exact-integer? value)
           (raise-asset-error "Expected integer for 'i8, got ~v" value))
         (unless (<= #x-80 value #x7f)
           (raise-asset-error "Value for i8 is out of range: ~a" value))]
    ['u16 (unless (exact-integer? value)
            (raise-asset-error "Expected integer for 'i16, got ~v" value))
          (unless (<= #x-8000 value #x7fff)
            (raise-asset-error "Value for i16 is out of range: ~a" value))]
    ['u32 (unless (exact-integer? value)
            (raise-asset-error "Expected integer for 'i32, got ~v" value))
          (unless (<= #x-80000000 value #x7fffffff)
            (raise-asset-error "Value for i32 is out of range: ~a" value))]
    ['u64 (unless (exact-integer? value)
            (raise-asset-error "Expected integer for 'i64, got ~v" value))
          (unless (<= #x-8000000000000000 value #x7fffffffffffffff)
            (raise-asset-error "Value for i64 is out of range: ~a" value))]

    ['f32 (unless (real? value)
            (raise-asset-error "Expected flonum for f32, got ~v" value))]
    ['f64 (unless (real? value)
            (raise-asset-error "Expected flonum for f64, got ~v" value))]

    ['bool (void)]

    ['string (unless (string? value)
               (raise-asset-error "Expected a string, got ~v" value))]

    [(cons 'vec item-type)
     (unless (vector? value)
       (raise-asset-error "Expected a vector, got ~v" value))
     (for ([e value]) (check-type e item-type asset-exists? retrieve-node))]

    [(list item-types ...)
     (unless (list? value)
       (raise-asset-error "Expected a list of values for tuple type, got ~v" value))
     (unless (= (length value) (length item-types))
       (raise-asset-error "Incorrect number of values for tuple type, expected ~a, got ~a"
                         (length item-types) (length value)))
     (for ([item value] [item-type item-types]) (check-type item item-type asset-exists? retrieve-node))]

    [(cons 'node node)
     (unless (and (list? value) (<= 1 (length value)) (symbol? (car value)))
       (raise-asset-error "Node instance must be a list starting with the node type, got ~v" value))
     (unless (or (eq? node '*) (eq? node (car value)))
       (raise-asset-error "Expected node instance of type ~v, got ~v" node (car value)))

     (let* ([node-data (retrieve-node (car value))]
            [parameters (cdr (or (assq 'parameters node-data)
                                 '(parameters . ())))])
       (unless (= (length parameters) (length (cdr value)))
         (raise-asset-error "Wrong number of parameters for node instance, expected ~a, got ~a"
                           (length parameters) (length (cdr value))))
       (for/or ([p parameters] [v (cdr value)]) (check-type v (cadr p) asset-exists? retrieve-node)))]

    ['node-list
     (unless (symbol? value)
       (raise-asset-error "Expected a symbol for node-list, got ~v" value))
     (unless (asset-exists? 'node-list value)
       (raise-asset-error "Node list ~a does not exist" value))]

    ['sequence
     (unless (symbol? value)
       (raise-asset-error "Expected a symbol for sequence, got ~v" value))
     (unless (asset-exists? 'sequence value)
       (raise-asset-error "Sequence list ~a does not exist" value))]

    [_ "Unknown type"]))


(define (validate-node asset-data asset-exists?)
  (define (validate-variable var)
    (unless (and (list? var) (= (length var) 2))
      (raise-asset-error "Variable must be a list with name and type, got ~v" var))
    (match-let ([(list name type) var])
      (unless (symbol? name)
        (raise-asset-error "Variable name must be a symbol, got ~v" name))
      (validate-type type asset-exists?)))

  (unless (list? asset-data)
    (raise-asset-error "Node asset must be an alist, got ~v" asset-data))

  (let ([parameters (cdr (or (assq 'parameters asset-data) '(parameters . ())))])
    (unless (or (not parameters) (list? parameters))
      (raise-asset-error "'parameters key must be a list of variables, got ~v" parameters))
    (when parameters
      (for ([p parameters]) (validate-variable p))))
  (let ([properties (cdr (or (assq 'properties asset-data) '(properties . ())))])
    (unless (or (not properties) (list? properties))
      (raise-asset-error "'properties key must be a list of variables, got ~v" properties))
    (when properties
      (for ([p properties]) (validate-variable p)))))


(define (validate-node-list asset-data asset-exists? retrieve-node)
  (unless (list? asset-data)
    (raise-asset-error "Node list must be a list, got ~v" asset-data))
  (for/or ([inst asset-data]) (check-type inst '(node . *) asset-exists? retrieve-node)))

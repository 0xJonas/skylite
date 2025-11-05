#lang racket

(provide validate-node validate-node-list)


(define (validate-type type asset-exists?)
  (match type
    [(or 'u8 'u16 'u32 'u64 'i8 'i16 'i32 'i64 'f32 'f64 'bool 'string 'project 'node-list 'sequence) (void)]
    [(cons 'vec item-type) (validate-type item-type asset-exists?)]
    [(cons 'node node)
     (unless (symbol? node)
       (raise-user-error 'validate-type "Node must be a symbol, got ~a" node))
     (unless (or (eq? node '*) (asset-exists? 'node node))
       (raise-user-error 'validate-type "Node ~a does not exist" node))]
    [(list item-types ...) (for ([item-type item-types]) (validate-type item-type asset-exists?))]
    [else (raise-user-error 'validate-type "Unknown type ~a" else)]))


(define (check-type value type asset-exists? retrieve-node)
  (validate-type type asset-exists?)

  (match type
    ['u8 (unless (exact-integer? value)
           (raise-user-error 'check-type "Expected integer for 'u8, got ~s" value))
         (unless (<= 0 value #xff)
           (raise-user-error 'check-type "Value for u8 is out of range: ~a" value))]
    ['u16 (unless (exact-integer? value)
            (raise-user-error 'check-type "Expected integer for 'u16, got ~s" value))
          (unless (<= 0 value #xffff)
            (raise-user-error 'check-type "Value for u16 is out of range: ~a" value))]
    ['u32 (unless (exact-integer? value)
            (raise-user-error 'check-type "Expected integer for 'u32, got ~s" value))
          (unless (<= 0 value #xffffffff)
            (raise-user-error 'check-type "Value for u32 is out of range: ~a" value))]
    ['u64 (unless (exact-integer? value)
            (raise-user-error 'check-type "Expected integer for 'u64, got ~s" value))
          (unless (<= 0 value #xffffffffffffffff)
            (raise-user-error 'check-type "Value for u64 is out of range: ~a" value))]

    ['u8 (unless (exact-integer? value)
           (raise-user-error 'check-type "Expected integer for 'i8, got ~s" value))
         (unless (<= #x-80 value #x7f)
           (raise-user-error 'check-type "Value for i8 is out of range: ~a" value))]
    ['u16 (unless (exact-integer? value)
            (raise-user-error 'check-type "Expected integer for 'i16, got ~s" value))
          (unless (<= #x-8000 value #x7fff)
            (raise-user-error 'check-type "Value for i16 is out of range: ~a" value))]
    ['u32 (unless (exact-integer? value)
            (raise-user-error 'check-type "Expected integer for 'i32, got ~s" value))
          (unless (<= #x-80000000 value #x7fffffff)
            (raise-user-error 'check-type "Value for i32 is out of range: ~a" value))]
    ['u64 (unless (exact-integer? value)
            (raise-user-error 'check-type "Expected integer for 'i64, got ~s" value))
          (unless (<= #x-8000000000000000 value #x7fffffffffffffff)
            (raise-user-error 'check-type "Value for i64 is out of range: ~a" value))]

    ['f32 (unless (real? value)
            (raise-user-error 'check-type "Expected flonum for f32, got ~s" value))]
    ['f64 (unless (real? value)
            (raise-user-error 'check-type "Expected flonum for f64, got ~s" value))]

    ['bool (void)]

    ['string (unless (string? value)
               (raise-user-error 'check-type "Expected a string, got ~s" value))]

    [(cons 'vec item-type)
     (unless (vector? value)
       (raise-user-error 'check-type "Expected a vector, got ~s" value))
     (for ([e value]) (check-type e item-type asset-exists? retrieve-node))]

    [(list item-types ...)
     (unless (list? value)
       (raise-user-error 'check-type "Expected a list of values for tuple type, got ~s" value))
     (unless (= (length value) (length item-types))
       (raise-user-error 'check-type "Incorrect number of values for tuple type, expected ~a, got ~a"
                         (length item-types) (length value)))
     (for ([item value] [item-type item-types]) (check-type item item-type asset-exists? retrieve-node))]

    [(cons 'node node)
     (unless (and (list? value) (<= 1 (length value)) (symbol? (car value)))
       (raise-user-error 'check-type "Node instance must be a list starting with the node type, got ~s" value))
     (unless (or (eq? node '*) (equal? (symbol->string node) (car value)))
       (raise-user-error 'check-type "Expected node instance of type ~s, got ~s" node (car value)))

     (let* ([node-data (retrieve-node (symbol->string (car value)))]
            [parameters (cdr (or (assq 'parameters node-data)
                                 '(parameters . ())))])
       (unless (= (length parameters) (length (cdr value)))
         (raise-user-error 'check-type "Wrong number of parameters for node instance, expected ~a, got ~a"
                           (length parameters) (length (cdr value))))
       (for/or ([p parameters] [v (cdr value)]) (check-type v (cadr p) asset-exists? retrieve-node)))]

    ['node-list
     (unless (symbol? value)
       (raise-user-error 'check-type "Expected a symbol for node-list, got ~s" value))
     (unless (asset-exists? 'node-list value)
       (raise-user-error 'check-type "Node list ~a does not exist" value))]

    ['sequence
     (unless (symbol? value)
       (raise-user-error 'check-type "Expected a symbol for sequence, got ~s" value))
     (unless (asset-exists? 'sequence value)
       (raise-user-error 'check-type "Sequence list ~a does not exist" value))]

    [_ "Unknown type"]))


(define (validate-node asset-data asset-exists?)
  (define (validate-variable var)
    (unless (and (list? var) (= (length var) 2))
      (raise-user-error 'validate-node "Variable must be a list with name and type, got ~s" var))
    (match-let ([(list name type) var])
      (unless (symbol? name)
        (raise-user-error 'validate-node "Variable name must be a symbol, got ~s" name))
      (validate-type type asset-exists?)))

  (unless (list? asset-data)
    (raise-user-error 'validate-node "Node asset must be an alist, got ~s" asset-data))

  (let ([parameters (cdr (or (assq 'parameters asset-data) '(parameters . ())))])
    (unless (or (not parameters) (list? parameters))
      (raise-user-error 'validate-node "'parameters key must be a list of variables, got ~s" parameters))
    (when parameters
      (for ([p parameters]) (validate-variable p))))
  (let ([properties (cdr (or (assq 'properties asset-data) '(properties . ())))])
    (unless (or (not properties) (list? properties))
      (raise-user-error 'validate-node "'properties key must be a list of variables, got ~s" properties))
    (when properties
      (for ([p properties]) (validate-variable p)))))


(define (validate-node-list asset-data asset-exists? retrieve-node)
  (unless (list? asset-data)
    (raise-user-error 'validate-node-list "Node list must be a list, got ~s" asset-data))
  (for/or ([inst asset-data]) (check-type inst '(node . *) asset-exists? retrieve-node)))

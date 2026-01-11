#lang racket

(require "./log-trace.rkt")
(provide validate-type refine-value
         (struct-out project-asset)
         (struct-out node)
         (struct-out sequence))

(struct project-asset (name globs))
(struct node (parameters properties))
(struct sequence (node script))


(define (validate-type type)
  (match type
    [(or 'u8 'u16 'u32 'u64 'i8 'i16 'i32 'i64 'f32 'f64 'bool 'string 'project 'node-list 'sequence) (void)]
    [(cons 'vec item-type) (validate-type item-type)]
    [(cons 'node node)
     (unless (symbol? node)
       (raise-asset-error "Node must be a symbol, got ~a" node))]
    [(list item-types ...) (for ([item-type item-types]) (validate-type item-type))]
    [else (raise-asset-error "Unknown type ~a" else)]))


(define (validate-primitive-typed-value type value)
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

    ['i8 (unless (exact-integer? value)
           (raise-asset-error "Expected integer for 'i8, got ~v" value))
         (unless (<= #x-80 value #x7f)
           (raise-asset-error "Value for i8 is out of range: ~a" value))]
    ['i16 (unless (exact-integer? value)
            (raise-asset-error "Expected integer for 'i16, got ~v" value))
          (unless (<= #x-8000 value #x7fff)
            (raise-asset-error "Value for i16 is out of range: ~a" value))]
    ['i32 (unless (exact-integer? value)
            (raise-asset-error "Expected integer for 'i32, got ~v" value))
          (unless (<= #x-80000000 value #x7fffffff)
            (raise-asset-error "Value for i32 is out of range: ~a" value))]
    ['i64 (unless (exact-integer? value)
            (raise-asset-error "Expected integer for 'i64, got ~v" value))
          (unless (<= #x-8000000000000000 value #x7fffffffffffffff)
            (raise-asset-error "Value for i64 is out of range: ~a" value))]

    ['f32 (unless (real? value)
            (raise-asset-error "Expected flonum for f32, got ~v" value))]
    ['f64 (unless (real? value)
            (raise-asset-error "Expected flonum for f64, got ~v" value))]

    ['bool (void)]

    ['string (unless (string? value)
               (raise-asset-error "Expected a string, got ~v" value))]))


(define (refine-value type value compute-id retrieve-node)
  (validate-type type)

  (match type
    [(cons 'vec item-type)
     (unless (vector? value)
       (raise-asset-error "Expected a vector, got ~v" value))
     (for/vector ([e value]) (refine-value item-type e compute-id retrieve-node))]

    [(list item-types ...)
     (unless (list? value)
       (raise-asset-error "Expected a list of values for tuple type, got ~v" value))
     (unless (= (length value) (length item-types))
       (raise-asset-error "Incorrect number of values for tuple type, expected ~a, got ~a"
                          (length item-types) (length value)))
     (for/list ([item value] [item-type item-types]) (refine-value item-type item compute-id retrieve-node))]

    [(cons 'node node)
     (unless (and (list? value) (<= 1 (length value)) (symbol? (car value)))
       (raise-asset-error "Node instance must be a list starting with the node type, got ~v" value))
     (unless (or (eq? node '*) (eq? node (car value)))
       (raise-asset-error "Expected node instance of type ~v, got ~v" node (car value)))

     (let* ([node (retrieve-node (car value))]
            [parameters (node-parameters node)])
       (unless (= (length parameters) (length (cdr value)))
         (raise-asset-error "Wrong number of parameters for node instance, expected ~a, got ~a"
                            (length parameters) (length (cdr value))))

       ; For node instances, type information is added to the parameters,
       ; so the type does not have to be retrieved again when serializing.
       (cons
        (car value)
        (for/list ([p parameters] [v (cdr value)])
          (let* ([type (cadr p)]
                 [value (refine-value type v compute-id retrieve-node)])
            (cons type value)))))]

    ['node-list
     (unless (symbol? value)
       (raise-asset-error "Expected a symbol for node-list, got ~v" value))
     (cons value (compute-id 'node-list value))]

    ['sequence
     (unless (symbol? value)
       (raise-asset-error "Expected a symbol for sequence, got ~v" value))
     (const value (compute-id 'sequence value))]

    [primitive
     (validate-primitive-typed-value primitive value)
     value]))

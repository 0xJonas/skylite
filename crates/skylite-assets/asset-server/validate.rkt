#lang racket

(require "./log-trace.rkt")
(provide validate-node validate-node-list validate-sequence)


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


(define (numeric-type? type)
  (memq type '(u8 u16 u32 u64 i8 i16 i32 i64 f32 f64)))


(define (validate-sequence asset-data asset-exists? retrieve-node)
  (unless (list? asset-data)
    (raise-user-error "Sequence asset must be an alist, got ~v" asset-data))
  (define target-node
    (let ([e (assq 'node asset-data)])
      (if e
          (retrieve-node (cdr e))
          (raise-user-error "Missing require key 'node"))))

  (define (get-property-type path)
    (for/fold ([acc target-node])
              ([segment (string-split (symbol->string path) ".")])
      ; Will fail if the previous segment did not resolve to a node.
      (unless (list? acc) (raise-asset-error "Unable to resolve property ~a" segment))
      (define property-list (assq 'properties acc))
      (unless property-list (raise-asset-error "Unable to resolve property ~a" segment))
      (define property (assq (string->symbol segment) (cdr property-list)))
      (unless property (raise-asset-error "Unable to resolve property ~a" segment))
      (match (cadr property)
        [(cons 'node name) (retrieve-node name)]
        [type type])))

  (define (validate-branch-condition condition)
    (match condition
      [(list op prop value)
       (unless (memq op '(= != < > <= >=))
         (raise-asset-error "Invalid comparison operation for branch condition: ~a" op))
       (define property-type (get-property-type prop))
       (when (and (memq op '(< > <= >=)) (not (numeric-type? property-type)))
         (raise-asset-error "Comparison is only allowed for numeric types"))
       (unless (check-type value property-type asset-exists? retrieve-node)
         (raise-asset-error "Value for 'branch' condition does not match property type ~a" property-type))]
      [(list '! prop)
       (define property-type (get-property-type prop))
       (unless (eq? property-type 'bool) (raise-asset-error "Branch if false is only allowed for bool properties"))]
      [prop
       #:when (symbol? prop)
       (define property-type (get-property-type prop))
       (unless (eq? property-type 'bool) (raise-asset-error "Branch if true is only allowed for bool properties"))]
      [_ (raise-asset-error "Invalid branch condition ~v" condition)]))

  (define (validate-instruction inst)
    (match inst
      [(list 'set prop value)
       (unless (symbol? prop) (raise-asset-error "Expected symbol for 'set' instruction, got ~v" prop))
       (define property-type (get-property-type prop))
       (unless (check-type value property-type asset-exists? retrieve-node)
         (raise-asset-error "Value for 'set' instruction does not match property type ~a" property-type))]
      [(list 'modify prop value)
       (unless (symbol? prop) (raise-asset-error "Expected symbol for 'set' instruction, got ~v" prop))
       (define property-type (get-property-type prop))
       (unless (numeric-type? property-type)
         (raise-asset-error "'modify' can only be used for fields with numeric types."))
       (unless (check-type value property-type asset-exists? retrieve-node)
         (raise-asset-error "Value for 'modify' instruction does not match property type ~a" property-type))]
      [(list 'branch condition target)
       (validate-branch-condition condition)
       (unless (symbol? target) (raise-asset-error "Target for 'branch' instruction must be a symbol, got ~v" target))]
      [(list 'jump target)
       (unless (symbol? target) (raise-asset-error "Target for 'jump' instruction must be a symbol, got ~v" target))]
      [(list 'call sub)
       (unless (symbol? sub) (raise-asset-error "Target for 'call' instruction must be a symbol, got ~v" sub))]
      [(list 'return) #t]
      [(list 'wait frames)
       (when (or (not (exact? frames)) (< frames 0))
         (raise-asset-error "'wait' instruction expects a positive integer, got ~v" frames))]
      [(list 'run-custom fname)
       (unless (symbol? fname)
         (raise-asset-error "'run-custom' instruction expects a symbol as function name, got ~v" fname))]
      [(list 'branch-custom fname target)
       (unless (symbol? fname)
         (raise-asset-error "'run-custom' instruction expects a symbol as function name, got ~v" fname))
       (unless (symbol? target) (raise-asset-error "Target for 'branch-custom' instruction must be a symbol, got ~v" target))]
      [_ (raise-asset-error "Unknown instruction ~v" inst)]))

  (define (validate-labels script)
    (define (forwards-label? item)
      (and (symbol? item) (string-prefix? "+" (symbol->string item))))
    (define (backwards-label? item)
      (and (symbol? item) (string-prefix? "-" (symbol->string item))))
    (define (get-jump-target inst)
      (match inst
        [(list 'branch _ target) target]
        [(list 'jump target) target]
        [(list 'branch-custom _ target) target]
        [_ #f]))

    (let lp ([script script]
             [known-labels '()]
             [pending-targets '()])
      (cond
        ; End of script
        [(null? script)
         (when (not (empty? pending-targets))
           (raise-asset-error "Jump targets not found: ~a" pending-targets))]
        ; Item is a new label
        [(symbol? (car script))
         (define label (car script))
         (when (and (memq label known-labels) (not (backwards-label? label)))
           (raise-asset-error "Duplicate label ~a" label))
         (lp (cdr script)
             (if (forwards-label? label) known-labels (cons label known-labels))
             (remq label pending-targets))]
        ; Item is an instruction
        [else
         (let ([target (get-jump-target (car script))])
           (define new-pending-targets
             (if target
                 (if (memq target known-labels)
                     pending-targets
                     (if (backwards-label? target)
                         (raise-asset-error "Backward jump target not found: ~a" target)
                         (cons target pending-targets)))
                 pending-targets))
           (lp (cdr script)
               known-labels
               new-pending-targets))])))

  (define (validate-script script)
    (for ([item script])
      (cond
        [(list? item) (validate-instruction item)]
        [(symbol? item) #t]
        [else (raise-asset-error "Invalid syntax in script: ~v" item)]))
    (validate-labels script))

  (define (validate-calls script subs)
    (for ([inst script])
      (match inst
        [(list 'call sub)
         (unless (memq sub subs)
           (raise-asset-error "Subroutine not found: ~a" sub))]
        [_ (void)])))

  (define sub-list (cdr (or (assq 'subs asset-data) '(subs . ()))))
  (unless (list? sub-list)
    (raise-asset-error "'subs key must be an alist, got ~v" sub-list))
  (define sub-names (map car sub-list))

  (for ([pair sub-list])
    (unless (pair? pair)
      (raise-asset-error "'subs key must be an alist, got ~v" sub-list))
    (unless (symbol? (car pair)) (raise-asset-error "Subroutine name must be a symbol, got ~v" (car pair)))
    (validate-script (cdr pair))
    (validate-calls (cdr pair) sub-names))

  (define main-script (assq 'script asset-data))
  (unless main-script
    (raise-asset-error "Missing required key 'script"))
  (validate-script (cdr main-script))
  (validate-calls (cdr main-script) sub-names))


(module+ test
  (require raco/testing)

  (define (expect-ok thunk)
    (with-handlers ([exn:asset? (lambda (e) (displayln (exn:asset-message e)) #f)])
      (thunk)))

  (define (retrieve-node node)
    (case node
      ['node1 '([properties . ([prop1 u8] [prop2 bool] [prop3 string] [prop4 (node . node2)])])]
      ['node2 '([properties . ([prop1 u8])])]))

  (define (asset-exists? name)
    (memq name '(node1 node2)))

  ; Basic parsing
  (define seq1
    '([node . node1]
      [subs . ([sub1 . ((return))])]
      [script
       . (- (set prop1 5)
            (modify prop1 5)
            (branch prop2 -)
            (branch (! prop2) -)
            (branch (= prop3 "test") -)
            (branch (!= prop3 "xyz") -)
            (branch (< prop1 10) -)
            (jump -)
            (call sub1)
            (run-custom test-fn)
            (branch-custom branch-fn -))]))
  (test-log! (expect-ok (lambda () (validate-sequence seq1 asset-exists? retrieve-node))))

  ; Nested properties
  (define seq2
    '([node . node1]
      [script
       . ((set prop1 5)
          (set prop4.prop1 10))]))
  (test-log! (expect-ok (lambda () (validate-sequence seq2 asset-exists? retrieve-node))))

  ; Complex labels
  (define seq3
    '([node . node1]
      [script
       . (- (jump -)
            -- label (wait 0)
            (jump --)
            (jump +)
            + (jump +)
            + (jump label))]))
  (test-log! (expect-ok (lambda () (validate-sequence seq3 asset-exists? retrieve-node)))))

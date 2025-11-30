#lang racket

(require "./log-trace.rkt")
(require "./types.rkt")

(provide refine-sequence)


(define (get-nested-property-type start path retrieve-node)
  (for/fold ([acc (retrieve-node start)])
            ([segment path])
    ; Will fail if the previous segment did not resolve to a node.
    (unless (node? acc) (raise-asset-error "Unable to resolve property ~a" segment))
    (define property (assq (string->symbol segment) (node-properties acc)))
    (unless property (raise-asset-error "Unable to resolve property ~a" segment))
    (match (cadr property)
      [(cons 'node name) (retrieve-node name)]
      [type type])))


; When a 'forwards' local label is used in a jump/branch instruction,
; the target is the first matching label when searching forwards from the instruction.
(define (forwards-label? item) (string-prefix? (symbol->string item) "+"))


; When a 'backwards' local label is used in a jump/branch instruction,
; the target is the first matching label when searching backwards from the instruction.
(define (backwards-label? item) (string-prefix? (symbol->string item) "-"))


(define (get-jump-target inst)
  (match inst
    [(list 'branch _ target) target]
    [(list 'jump target) target]
    [(list 'branch-custom _ target) target]
    [_ #f]))


(define (set-jump-target inst target)
  (match inst
    [(list 'branch cond _) (list 'branch cond target)]
    [(list 'jump _) (list 'jump target)]
    [(list 'branch-custom fname _) (list 'branch-custom fname target)]
    [_ inst]))


(define (validate-labels script)
  (let lp ([script script]
           [known-labels (set)]
           [pending-targets (set)])
    (cond
      ; End of script
      [(null? script)
       (when (not (set-empty? pending-targets))
         (raise-asset-error "Jump targets not found: ~a" pending-targets))]
      ; Item is a new label
      [(symbol? (car script))
       (define label (car script))
       (when (and (set-member? known-labels label) (not (backwards-label? label)))
         (raise-asset-error "Duplicate label ~a" label))

       (lp (cdr script)
           (if (forwards-label? label)
               known-labels
               (set-add known-labels label))
           (set-remove pending-targets label))]
      ; Item is an instruction
      [else
       (let ([target (get-jump-target (car script))])
         (define new-pending-targets
           (if (and target (not (set-member? known-labels target)))
               (if (backwards-label? target)
                   (raise-asset-error "Jump target not found: ~a" target)
                   (set-add pending-targets target))
               pending-targets))
         (lp (cdr script)
             known-labels
             new-pending-targets))])))


(define (validate-calls script subs)
  (for ([item script])
    (match item
      [(list 'call sub)
       (unless (memq sub subs)
         (raise-asset-error "Call to undefined subroutine ~a" sub))]
      [_ (void)])))


(define (rename-subroutine sub) (string-append "sub-" (symbol->string sub)))


(define (rename-labels script name)
  ; Important: This changes the to labels from symbols to strings.
  (define (rename-single label c)
    (cond
      [(forwards-label? label) (string-append (symbol->string name) "-f-" (symbol->string label) "-" (number->string c))]
      [(backwards-label? label) (string-append (symbol->string name) "-b-" (symbol->string label) "-" (number->string c))]
      [else (string-append (symbol->string name) "-l-" (symbol->string label))]))

  (define (symbol-only target) (if (symbol? target) target #f))

  (define (rename-directional-labels script detect)
    (let lp ([script script]
             [labels (make-immutable-hash)] ; Maps all current labels to their new names.
             [item-count 0]) ; Used when renaming local labels, to disambiguate labels with the same name.
      (cond
        ; End of script
        [(null? script) '()]
        ; Already processed label
        [(string? (car script)) (cons (car script) (lp (cdr script) labels (+ 1 item-count)))]
        ; Unprocessed label
        [(symbol? (car script))
         (define label (car script))
         (define new-label (if (detect label) (rename-single label item-count) label))
         (cons
          new-label
          (lp (cdr script)
              (if (detect label)
                  (hash-set labels label (rename-single label item-count))
                  labels)
              (+ 1 item-count)))]
        ; Instruction
        [(list? (car script))
         (define inst (car script))
         ; Only check the target if it is still a symbol.
         ; If the target is a string, it has already been processed and should not
         ; checked again to avoid potential naming collisions.
         (define target (symbol-only (get-jump-target inst)))
         (define new-inst
           (if (and target (detect target))
               (set-jump-target inst (hash-ref labels target))
               inst))
         (cons
          new-inst
          (lp (cdr script)
              labels
              (+ 1 item-count)))])))

  (define (rename-normal-labels script)
    (for/list ([item script])
      (cond
        ; Unprocessed label
        [(symbol? item)
         (if (not (or (backwards-label? item) (forwards-label? item)))
             (rename-single item 0)
             item)]
        ; Instruction
        [(list? item)
         (define target (symbol-only (get-jump-target item)))
         (if (and target (not (backwards-label? target)) (not (forwards-label? target)))
             (set-jump-target item (rename-single target 0))
             item)]
        [else item])))

  (define (rename-calls script)
    (for/list ([item script])
      (match item
        [(list 'call target) (list 'call (rename-subroutine target))]
        [item item])))

  (define normal-renamed (rename-normal-labels script))
  (define backwards-renamed (rename-directional-labels normal-renamed backwards-label?))
  (define forwards-renamed (reverse (rename-directional-labels (reverse backwards-renamed) forwards-label?)))
  (define calls-renamed (rename-calls forwards-renamed))

  calls-renamed)


(define (merge-scripts scripts)
  (define prepared-scripts
    (for/list ([pair scripts])
      (append
       (list (rename-subroutine (car pair)))
       (cdr pair)
       (if (match (last (cdr pair)) [(list 'return) #f] [(list 'jump _) #f] [_ #t])
           '((return))
           '()))))
  (apply append prepared-scripts))


(define (numeric-type? type)
  (memq type '(u8 u16 u32 u64 i8 i16 i32 i64 f32 f64)))


(define (refine-sequence asset-data asset-exists? retrieve-node)
  (unless (list? asset-data)
    (raise-user-error "Sequence asset must be an alist, got ~v" asset-data))
  (define target-node
    (let ([e (assq 'node asset-data)])
      (or (cdr e) (raise-user-error "Missing require key 'node"))))

  ; Validate correct syntax and add type information to immediate values.
  (define (refine-branch-condition condition)
    (match condition
      [(list op prop value)
       (unless (memq op '(= != < > <= >=))
         (raise-asset-error "Invalid comparison operation for branch condition: ~a" op))
       (define path (string-split (symbol->string prop) "."))
       (define property-type (get-nested-property-type target-node path retrieve-node))
       (when (and (memq op '(< > <= >=)) (not (numeric-type? property-type)))
         (raise-asset-error "Comparison is only allowed for numeric types"))
       `(,op ,path (,property-type . ,(refine-value property-type value asset-exists? property-type)))]

      [(list '! prop)
       (define path (string-split (symbol->string prop) "."))
       (define property-type (get-nested-property-type target-node path retrieve-node))
       (unless (eq? property-type 'bool) (raise-asset-error "Branch if false is only allowed for bool properties"))
       `(! ,path)]

      [(or prop (list prop))
       #:when (symbol? prop)
       (define path (string-split (symbol->string prop) "."))
       (define property-type (get-nested-property-type target-node path retrieve-node))
       (unless (eq? property-type 'bool) (raise-asset-error "Branch if true is only allowed for bool properties"))
       path]

      [_ (raise-asset-error "Invalid branch condition ~v" condition)]))

  ; Validate correct syntax and add type information to immediate values.
  (define (refine-instruction inst)
    (match inst
      [(list 'set prop value)
       (unless (symbol? prop) (raise-asset-error "Expected symbol for 'set' instruction, got ~v" prop))
       (define path (string-split (symbol->string prop) "."))
       (define property-type (get-nested-property-type target-node path retrieve-node))
       `(set ,path (,property-type . ,(refine-value property-type value asset-exists? property-type)))]

      [(list 'modify prop value)
       (unless (symbol? prop) (raise-asset-error "Expected symbol for 'set' instruction, got ~v" prop))
       (define path (string-split (symbol->string prop) "."))
       (define property-type (get-nested-property-type target-node path retrieve-node))
       (unless (numeric-type? property-type)
         (raise-asset-error "'modify' can only be used for fields with numeric types."))
       `(modify ,path (,property-type . ,(refine-value property-type value asset-exists? property-type)))]

      [(list 'branch condition target)
       (unless (symbol? target) (raise-asset-error "Target for 'branch' instruction must be a symbol, got ~v" target))
       `(branch ,(refine-branch-condition condition) ,target)]

      [(list 'jump target)
       (unless (symbol? target) (raise-asset-error "Target for 'jump' instruction must be a symbol, got ~v" target))
       inst]

      [(list 'call sub)
       (unless (symbol? sub) (raise-asset-error "Target for 'call' instruction must be a symbol, got ~v" sub))
       inst]

      [(list 'return) inst]

      [(list 'wait frames)
       (when (or (not (exact? frames)) (< frames 0))
         (raise-asset-error "'wait' instruction expects a positive integer, got ~v" frames))
       inst]

      [(list 'run-custom fname)
       (unless (symbol? fname)
         (raise-asset-error "'run-custom' instruction expects a symbol as function name, got ~v" fname))
       inst]

      [(list 'branch-custom fname target)
       (unless (symbol? fname)
         (raise-asset-error "'run-custom' instruction expects a symbol as function name, got ~v" fname))
       (unless (symbol? target) (raise-asset-error "Target for 'branch-custom' instruction must be a symbol, got ~v" target))
       inst]

      [_ (raise-asset-error "Unknown instruction ~v" inst)]))

  (define sub-list (cdr (or (assq 'subs asset-data) '(subs . ()))))
  (unless (list? sub-list)
    (raise-asset-error "'subs key must be an alist, got ~v" sub-list))
  (define sub-names (map car sub-list))
  (when (memq '$main sub-names)
    (raise-asset-error "'$main is not allowed as a subroutine name"))

  (define (refine-script script name)
    (define out
      (for/list ([item script])
        (cond
          [(list? item) (refine-instruction item)]
          [(symbol? item) item]
          [else (raise-asset-error "Unknown syntax in script: ~v" item)])))
    (validate-labels out)
    (validate-calls out sub-names)
    (rename-labels out name))

  (define refined-subs
    (for/list ([pair sub-list])
      (unless (pair? pair)
        (raise-asset-error "'subs key must be an alist, got ~v" sub-list))
      (unless (symbol? (car pair)) (raise-asset-error "Subroutine name must be a symbol, got ~v" (car pair)))
      (cons (car pair) (refine-script (cdr pair) (car pair)))))

  (define main-script (assq 'script asset-data))
  (unless main-script
    (raise-asset-error "Missing required key 'script"))
  (define main-refined (refine-script (cdr main-script) '$main))

  (define full-script (merge-scripts (cons (cons '$main main-refined) refined-subs)))

  (sequence target-node full-script))


(module+ test
  (require raco/testing)

  (define (retrieve-node name)
    (case name
      [(node1) (node '() '([prop1 u8] [prop2 bool] [prop3 string] [prop4 (node . node2)]))]
      [(node2) (node '() '([prop1 u8]))]))

  (define (asset-exists? name)
    (memq name '(node1 node2)))

  ; Basic refining
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
  (define refined1
    '("sub-$main"
      "$main-b---0"
      (set ("prop1") (u8 . 5))
      (modify ("prop1") (u8 . 5))
      (branch ("prop2") "$main-b---0")
      (branch (! ("prop2")) "$main-b---0")
      (branch (= ("prop3") (string . "test")) "$main-b---0")
      (branch (!= ("prop3") (string . "xyz")) "$main-b---0")
      (branch (< ("prop1") (u8 . 10)) "$main-b---0")
      (jump "$main-b---0")
      (call "sub-sub1")
      (run-custom test-fn)
      (branch-custom branch-fn "$main-b---0")
      (return)
      "sub-sub1"
      (return)))

  (test-log! (equal? (sequence-script (refine-sequence seq1 asset-exists? retrieve-node)) refined1))

  ; Nested properties
  (define seq2
    '([node . node1]
      [script
       . ((set prop1 5)
          (set prop4.prop1 10))]))
  (define refined2
    '("sub-$main"
      (set ("prop1") (u8 . 5))
      (set ("prop4" "prop1") (u8 . 10))
      (return)))

  (test-log! (equal? (sequence-script (refine-sequence seq2 asset-exists? retrieve-node)) refined2))

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
  (define refined3
    '("sub-$main"
      "$main-b---0"
      (jump "$main-b---0")
      "$main-b----2" "$main-l-label"
      (wait 0)
      (jump "$main-b----2")
      (jump "$main-f-+-3")
      "$main-f-+-3"
      (jump "$main-f-+-1")
      "$main-f-+-1"
      (jump "$main-l-label")))

  (test-log! (equal? (sequence-script (refine-sequence seq3 asset-exists? retrieve-node)) refined3)))

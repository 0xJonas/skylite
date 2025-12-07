#lang racket

(require "./log-trace.rkt")
(require "./types.rkt")

(provide refine-sequence)


(define (process-property-access start path retrieve-node)
  (let lp ([path path]
           [node-name start]
           [push-offset-ops '()])
    (define segment (car path))
    (define current-node (retrieve-node node-name))
    (define property (assq (string->symbol segment) (node-properties current-node)))
    (unless property (raise-asset-error "Unable to resolve property ~a" segment))
    (define prop-name (car property))
    (define prop-type (cadr property))
    (define inst `(push-offset ,(symbol->string node-name) ,(symbol->string prop-name)))
    (if (null? (cdr path))
        (values (reverse (cons inst push-offset-ops)) prop-type)
        (if (eq? (car prop-type) 'node)
            (lp (cdr path) (cdr prop-type) (cons inst push-offset-ops))
            (raise-asset-error "Intermediate property ~a is not a node" prop-name)))))


; When a 'forwards' local label is used in a jump/branch instruction,
; the target is the first matching label when searching forwards from the instruction.
(define (forwards-label? item) (string-prefix? (symbol->string item) "+"))


; When a 'backwards' local label is used in a jump/branch instruction,
; the target is the first matching label when searching backwards from the instruction.
(define (backwards-label? item) (string-prefix? (symbol->string item) "-"))


(define (get-jump-target inst)
  (match inst
    [(list 'branch-if-true target) target]
    [(list 'branch-if-false target) target]
    [(list 'branch-uint _ _ target) target]
    [(list 'branch-sint _ _ target) target]
    [(list 'branch-f32 _ _ target) target]
    [(list 'branch-f64 _ _ target) target]
    [(list 'jump target) target]
    [(list 'branch-custom _ target) target]
    ; call instructions are not handled in this step
    [_ #f]))


(define (set-jump-target inst target)
  (match inst
    [(list 'branch-if-true _) (list 'branch-if-true target)]
    [(list 'branch-if-false _) (list 'branch-if-false target)]
    [(list 'branch-uint op value _) (list 'branch-uint op value target)]
    [(list 'branch-sint op value _) (list 'branch-sint op value target)]
    [(list 'branch-f32 op value _) (list 'branch-f32 op value target)]
    [(list 'branch-f64 op value _) (list 'branch-f64 op value target)]
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


(define (resolve-labels script)
  (define label->pos
    (make-immutable-hash
     (let lp ([script script]
              [pos 0]
              [alist '()])
       (cond
         [(null? script) alist]
         [(string? (car script)) (lp (cdr script) pos (cons (cons (car script) pos) alist))]
         [else (lp (cdr script) (+ pos 1) alist)]))))
  (for/list ([item script] #:when (list? item))
    (match item
      [(list 'branch-if-true target) `(branch-if-true ,(hash-ref label->pos target))]
      [(list 'branch-if-false target) `(branch-if-false ,(hash-ref label->pos target))]
      [(list 'branch-uint op value target) `(branch-uint ,op ,value ,(hash-ref label->pos target))]
      [(list 'branch-sint op value target) `(branch-sint ,op ,value ,(hash-ref label->pos target))]
      [(list 'branch-f32 op value target) `(branch-f32 ,op ,value ,(hash-ref label->pos target))]
      [(list 'branch-f64 op value target) `(branch-f64 ,op ,value ,(hash-ref label->pos target))]
      [(list 'jump target) `(jump ,(hash-ref label->pos target))]
      [(list 'call target) `(call ,(hash-ref label->pos target))]
      [(list 'branch-custom fname target) `(branch-custom ,fname ,(hash-ref label->pos target))]
      [inst inst])))


(define (numeric-type? type)
  (memq type '(u8 u16 u32 u64 i8 i16 i32 i64 f32 f64)))


(define (refine-sequence asset-data asset-exists? retrieve-node)
  (unless (list? asset-data)
    (raise-user-error "Sequence asset must be an alist, got ~v" asset-data))
  (define target-node
    (let ([e (assq 'node asset-data)])
      (or (cdr e) (raise-user-error "Missing require key 'node"))))

  (define (compile-set prop value)
    (unless (symbol? prop) (raise-asset-error "Expected symbol for 'set' instruction, got ~v" prop))
    (define path (string-split (symbol->string prop) "."))
    (define-values (push-offset-ops prop-type) (process-property-access target-node path retrieve-node))
    (define refined-value (refine-value prop-type value asset-exists? retrieve-node))
    (append push-offset-ops
            ; set-string is a separate instruction, because String in Rust
            ; is heap-allocated, so it must be handled differently than other base types.
            (if (eq? prop-type 'string)
                (list `(set-string ,refined-value))
                (list `(set (,prop-type . ,refined-value))))))

  (define (compile-modify prop value)
    (unless (symbol? prop) (raise-asset-error "Expected symbol for 'modify' instruction, got ~v" prop))
    (define path (string-split (symbol->string prop) "."))
    (define-values (push-offset-ops prop-type) (process-property-access target-node path retrieve-node))
    (unless (numeric-type? prop-type)
      (raise-asset-error "'modify' can only be used for fields with numeric types."))
    (define refined-value (refine-value prop-type value asset-exists? retrieve-node))
    (append push-offset-ops
            (case prop-type
              [(f32) (list `(modify-f32 ,refined-value))]
              [(f64) (list `(modify-f64 ,refined-value))]
              [else (list `(modify (,prop-type . ,refined-value)))])))

  (define (compile-branch condition target)
    (define (comparison->byte comp)
      (case comp [(=) 2] [(!=) 3] [(<) 4] [(>) 5] [(<=) 6] [(>=) 7]))

    (unless (symbol? target) (raise-asset-error "Target for 'branch' instruction must be a symbol, got ~v" target))
    (match condition
      [(list op prop value)
       (unless (memq op '(= != < > <= >=))
         (raise-asset-error "Invalid comparison operation for branch condition: ~a" op))
       (define path (string-split (symbol->string prop) "."))
       (define-values (push-offset-ops prop-type) (process-property-access target-node path retrieve-node))
       (unless (numeric-type? prop-type)
         (raise-asset-error "Comparison is only allowed for numeric types"))
       (define refined-value (refine-value prop-type value asset-exists? retrieve-node))
       (append push-offset-ops
               (case prop-type
                 [(u8 u16 u32 u64) (list `(branch-uint ,(comparison->byte op) (,prop-type . ,refined-value) ,target))]
                 [(i8 i16 i32 i64) (list `(branch-sint ,(comparison->byte op) (,prop-type . ,refined-value) ,target))]
                 [(f32) (list `(branch-f32 ,(comparison->byte op) ,refined-value ,target))]
                 [(f64) (list `(branch-f64 ,(comparison->byte op) ,refined-value ,target))]))]

      [(list '! prop)
       (define path (string-split (symbol->string prop) "."))
       (define-values (push-offset-ops prop-type) (process-property-access target-node path retrieve-node))
       (unless (eq? prop-type 'bool) (raise-asset-error "Branch if false is only allowed for bool properties"))
       (append push-offset-ops
               (list `(branch-if-false ,target)))]

      [(or prop (list prop))
       #:when (symbol? prop)
       (define path (string-split (symbol->string prop) "."))
       (define-values (push-offset-ops prop-type) (process-property-access target-node path retrieve-node))
       (unless (eq? prop-type 'bool) (raise-asset-error "Branch if true is only allowed for bool properties"))
       (append push-offset-ops
               (list `(branch-if-true ,target)))]

      [_ (raise-asset-error "Invalid branch condition ~v" condition)]))

  ; Validate correct syntax and add type information to immediate values.
  (define (compile-instruction inst)
    (match inst
      [(list 'set prop value) (compile-set prop value)]

      [(list 'modify prop value) (compile-modify prop value)]

      [(list 'branch condition target) (compile-branch condition target)]

      [(list 'jump target)
       (unless (symbol? target) (raise-asset-error "Target for 'jump' instruction must be a symbol, got ~v" target))
       (list inst)]

      [(list 'call sub)
       (unless (symbol? sub) (raise-asset-error "Target for 'call' instruction must be a symbol, got ~v" sub))
       (list inst)]

      [(list 'return) (list inst)]

      [(list 'wait frames)
       (when (or (not (exact? frames)) (< frames 0))
         (raise-asset-error "'wait' instruction expects a positive integer, got ~v" frames))
       (list inst)]

      [(list 'run-custom fname)
       (unless (symbol? fname)
         (raise-asset-error "'run-custom' instruction expects a symbol as function name, got ~v" fname))
       (list inst)]

      [(list 'branch-custom fname target)
       (unless (symbol? fname)
         (raise-asset-error "'run-custom' instruction expects a symbol as function name, got ~v" fname))
       (unless (symbol? target) (raise-asset-error "Target for 'branch-custom' instruction must be a symbol, got ~v" target))
       (list inst)]

      [_ (raise-asset-error "Unknown instruction ~v" inst)]))

  (define sub-list (cdr (or (assq 'subs asset-data) '(subs . ()))))
  (unless (list? sub-list)
    (raise-asset-error "'subs key must be an alist, got ~v" sub-list))
  (define sub-names (map car sub-list))
  (when (memq '$main sub-names)
    (raise-asset-error "'$main is not allowed as a subroutine name"))

  (define (compile-script script name)
    (define out
      (apply append
             (for/list ([item script])
               (cond
                 [(list? item) (compile-instruction item)]
                 [(symbol? item) (list item)]
                 [else (raise-asset-error "Unknown syntax in script: ~v" item)]))))
    (validate-labels out)
    (validate-calls out sub-names)
    (rename-labels out name))

  (define subroutines
    (for/list ([pair sub-list])
      (unless (pair? pair)
        (raise-asset-error "'subs key must be an alist, got ~v" sub-list))
      (unless (symbol? (car pair)) (raise-asset-error "Subroutine name must be a symbol, got ~v" (car pair)))
      pair))
  (define main-script (assq 'script asset-data))
  (unless main-script
    (raise-asset-error "Missing required key 'script"))

  (define scripts (cons (cons '$main main-script) subroutines))
  (define compiled-scripts (for/list ([pair scripts]) (cons (car pair) (compile-script (cdr pair) (car pair)))))
  (define merged-script (merge-scripts compiled-scripts))
  (define resolved-script (resolve-labels merged-script))

  (sequence target-node resolved-script))


(module+ test
  (require raco/testing)

  (define (retrieve-node name)
    (case name
      [(node1) (node '() '([prop1 u8] [prop2 bool] [prop3 string] [prop4 (node . node2)] [prop5 f32] [prop6 f64] [prop7 i8]))]
      [(node2) (node '() '([prop1 u8]))]))

  (define (asset-exists? name)
    (memq name '(node1 node2)))

  ; Basic refining
  (define seq1
    '([node . node1]
      [subs . ([sub1 . ((return))])]
      [script
       . (- (set prop1 5)
            (set prop3 "test")
            (modify prop1 5)
            (modify prop5 1.0)
            (modify prop6 -1.0)
            (branch prop2 -)
            (branch (! prop2) -)
            (branch (= prop1 5) -)
            (branch (< prop7 6) -)
            (branch (> prop5 5.0) -)
            (branch (>= prop6 -5.0) -)
            (jump -)
            (call sub1)
            (run-custom test-fn)
            (branch-custom branch-fn -))]))
  (define refined1
    '((push-offset "node1" "prop1") ; 0
      (set (u8 . 5))
      (push-offset "node1" "prop3")
      (set-string "test")
      (push-offset "node1" "prop1")
      (modify (u8 . 5))             ; 5
      (push-offset "node1" "prop5")
      (modify-f32 1.0)
      (push-offset "node1" "prop6")
      (modify-f64 -1.0)
      (push-offset "node1" "prop2") ; 10
      (branch-if-true 0)
      (push-offset "node1" "prop2")
      (branch-if-false 0)
      (push-offset "node1" "prop1")
      (branch-uint 2 (u8 . 5) 0)    ; 15
      (push-offset "node1" "prop7")
      (branch-sint 4 (i8 . 6) 0)
      (push-offset "node1" "prop5")
      (branch-f32 5 5.0 0)
      (push-offset "node1" "prop6") ; 20
      (branch-f64 7 -5.0 0)
      (jump 0)
      (call 27)
      (run-custom test-fn)
      (branch-custom branch-fn 0)   ; 25
      (return)
      (return)))

  (test-log! (equal? (sequence-script (refine-sequence seq1 asset-exists? retrieve-node)) refined1))

  ; Nested properties
  (define seq2
    '([node . node1]
      [script
       . ((set prop1 5)
          (set prop4.prop1 10))]))
  (define refined2
    '((push-offset "node1" "prop1")
      (set (u8 . 5))
      (push-offset "node1" "prop4")
      (push-offset "node2" "prop1")
      (set (u8 . 10))
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
    '((jump 0)
      (wait 0)
      (jump 1)
      (jump 4)
      (jump 5)
      (jump 1)))

  (test-log! (equal? (sequence-script (refine-sequence seq3 asset-exists? retrieve-node)) refined3)))

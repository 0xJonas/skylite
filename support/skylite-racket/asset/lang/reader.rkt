#lang racket

(require syntax/strip-context)
(provide (rename-out [skylite-read read]
                     [skylite-read-syntax read-syntax]))

(define (skylite-read in)
  (syntax->datum (skylite-read-syntax "anonymous" in)))

(define (asset-type? type)
  (memq type '(project node node-list sequence)))

(define (skylite-read-syntax source-name in)
  (let* ([asset-file (path->bytes (file-name-from-path source-name))]
         [asset-extension (or (path-get-extension source-name) #"")]
         [asset-name (string->symbol
                      (bytes->string/utf-8
                       (subbytes asset-file 0 (- (bytes-length  asset-file)
                                                 (bytes-length asset-extension)))))]
         [asset-type-syntax (read-syntax source-name in)]
         [asset-type (eval (syntax->datum asset-type-syntax))]
         [forms (for/list (#:when (not (eof-object? (peek-byte in))))
                  (read-syntax source-name in))]
         [forms-rev (reverse forms)]
         [out-asset (car forms-rev)]
         [out-defs (reverse (cdr forms-rev))])
    (unless (asset-type? asset-type)
      (raise-syntax-error "#lang skylite/asset"
                          (format "Not a valid asset type: ~a" asset-type)
                          asset-type-syntax))
    (strip-context
     #`(module ignored racket
         (provide skylite-assets)
         #,@out-defs
         (define skylite-assets `([#,asset-name . ([type . #,asset-type]
                                                   [get . ,(lambda () #,out-asset)])]))))))

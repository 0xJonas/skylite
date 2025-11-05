#lang racket

(require "./project.rkt")

(provide serialize-obj deserialize-obj serialize-node serialize-node-list)


(define (serialize-type out type)
  (match type
    ['u8 (write-byte 0 out)]
    ['u16 (write-byte 1 out)]
    ['u32 (write-byte 2 out)]
    ['u64 (write-byte 3 out)]
    ['i8 (write-byte 4 out)]
    ['i16 (write-byte 5 out)]
    ['i32 (write-byte 6 out)]
    ['i64 (write-byte 7 out)]
    ['f32 (write-byte 8 out)]
    ['f64 (write-byte 9 out)]
    ['bool (write-byte 10 out)]
    ['string (write-byte 11 out)]
    [(cons 'vec item-type) (write-byte 12 out) (serialize-type out item-type)]
    [(list item-types ...) (write-byte 13 out) (for ([item-type item-types]) (serialize-type item-type))]
    ['project (write-byte 14 out)]
    [(cons 'node node-name) (write-byte 15 out) (serialize-obj out 'string (symbol->string node-name))]
    ['node-list (write-byte 16 out)]
    ['sequence (write-byte 17 out)]))


(define (serialize-obj out type value)
  (match type
    ['type (serialize-type out value)]
    ['u8 (write-bytes (integer->integer-bytes value 1 #f #f) out)]
    ['u16 (write-bytes (integer->integer-bytes value 2 #f #f) out)]
    ['u32 (write-bytes (integer->integer-bytes value 4 #f #f) out)]
    ['u64 (write-bytes (integer->integer-bytes value 8 #f #f) out)]
    ['i8 (write-bytes (integer->integer-bytes value 1 #t #f) out)]
    ['i16 (write-bytes (integer->integer-bytes value 2 #t #f) out)]
    ['i32 (write-bytes (integer->integer-bytes value 4 #t #f) out)]
    ['i64 (write-bytes (integer->integer-bytes value 8 #t #f) out)]
    ['f32 (write-bytes (real->floating-point-bytes value 4 #f) out)]
    ['f64 (write-bytes (real->floating-point-bytes value 8 #f) out)]
    ['bool (write-byte (if value 1 0) out)]
    ['string (let ([data (string->bytes/utf-8 value)])
               (serialize-obj out 'u32 (bytes-length data))
               (write-bytes data out))]
    [(cons 'vec item-type)
     (serialize-obj out 'u32 (vector-length value))
     (for ([item value]) (serialize-obj out item-type item))]
    [(list item-types ...)
     (for ([item-type item-types] [item value])
       (serialize-obj out item-type item))]
    ['project (serialize-obj out 'string (symbol->string value))]
    [(cons 'node name)
     (define-values (_ asset-data) (retrieve-asset 'node (symbol->string name)))
     (define parameters (cdr (or (assq 'parameters asset-data) '(parameters . ()))))
     (for ([p parameters] [v (cdr value)])
       (serialize-obj out 'type (cadr p))
       (serialize-obj out (cadr p) v))]
    ['node-list (serialize-obj out 'string (symbol->string value))]
    ['sequence (serialize-obj out 'string (symbol->string value))])
  (void))


(define (deserialize-obj in type)
  (match type
    ['u8 (integer-bytes->integer (read-bytes 1 in) #f #f)]
    ['u16 (integer-bytes->integer (read-bytes 2 in) #f #f)]
    ['u32 (integer-bytes->integer (read-bytes 4 in) #f #f)]
    ['u64 (integer-bytes->integer (read-bytes 8 in) #f #f)]
    ['i8 (integer-bytes->integer (read-bytes 1 in) #t #f)]
    ['i16 (integer-bytes->integer (read-bytes 2 in) #t #f)]
    ['i32 (integer-bytes->integer (read-bytes 4 in) #t #f)]
    ['i64 (integer-bytes->integer (read-bytes 8 in) #t #f)]
    ['f32 (floating-point-bytes->real (read-bytes 4 in) #f)]
    ['f64 (floating-point-bytes->real (read-bytes 8 in) #f)]
    ['bool (not (zero? (read-byte in)))]
    ['string (let* ([len (deserialize-obj in 'u32)]
                    [data (read-bytes len in)])
               (bytes->string/utf-8 data))]
    [(cons 'vec item-type)
     (let ([len (deserialize-obj in 'u32)])
       (for/vector ([_ (build-list len values)]) (deserialize-obj in item-type)))]
    [(list item-types ...) (for/list ([item-type item-types]) (deserialize-obj in item-type))]))


(define (serialize-node out asset-data)
  (define param-pair (assq 'parameters asset-data))
  (if param-pair
      (let ([parameters (cdr param-pair)])
        (serialize-obj out 'u32 (length parameters))
        (for ([var parameters])
          (serialize-obj out 'string (symbol->string (car var)))
          (serialize-type out (cadr var))))
      (serialize-obj out 'u32 0))

  (define prop-pair (assq 'properties asset-data))
  (if prop-pair
      (let ([properties (cdr prop-pair)])
        (serialize-obj out 'u32 (length properties))
        (for ([var properties])
          (serialize-obj out 'string (symbol->string (car var)))
          (serialize-type out (cadr var))))
      (serialize-obj out 'u32 0)))


(define (serialize-node-list out asset-data)
  (serialize-obj out 'u32 (length asset-data))
  (for ([inst asset-data])
    (serialize-obj out 'string (symbol->string (car inst)))
    (serialize-obj out 'u32 (length (cdr inst)))
    (serialize-obj out (cons 'node (car inst)) inst)))


(module+ test
  (require raco/testing)

  (define out (open-output-bytes))

  (serialize-obj out 'u8 5)
  (serialize-obj out 'u16 10)
  (serialize-obj out 'u32 15)
  (serialize-obj out 'u64 20)
  (serialize-obj out 'i8 -5)
  (serialize-obj out 'i16 -10)
  (serialize-obj out 'i32 -15)
  (serialize-obj out 'i64 -20)
  (serialize-obj out 'bool #f)
  (serialize-obj out 'string "test")
  (serialize-obj out '(vec . i16) #(1 2 3 4 5))
  (serialize-obj out '(string u8) '("a" 5))

  (define data (get-output-bytes out))
  (test-log!
   (equal? data
           (bytes 5 10 0 15 0 0 0 20 0 0 0 0 0 0 0 251 246 255 241 255 255 255
                  236 255 255 255 255 255 255 255 0 4 0 0 0 116 101 115 116 5 0 0
                  0 1 0 2 0 3 0 4 0 5 0 1 0 0 0 97 5)))

  (define in (open-input-bytes data))

  (test-log! (= (deserialize-obj in 'u8) 5))
  (test-log! (= (deserialize-obj in 'u16) 10))
  (test-log! (= (deserialize-obj in 'u32) 15))
  (test-log! (= (deserialize-obj in 'u64) 20))
  (test-log! (= (deserialize-obj in 'i8) -5))
  (test-log! (= (deserialize-obj in 'i16) -10))
  (test-log! (= (deserialize-obj in 'i32) -15))
  (test-log! (= (deserialize-obj in 'i64) -20))
  (test-log! (equal? (deserialize-obj in 'bool) #f))
  (test-log! (equal? (deserialize-obj in 'string) "test"))
  (test-log! (equal? (deserialize-obj in '(vec . i16)) #(1 2 3 4 5)))
  (test-log! (equal? (deserialize-obj in '(string u8)) '("a" 5))))

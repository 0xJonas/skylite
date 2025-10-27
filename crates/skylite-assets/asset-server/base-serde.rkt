#lang racket

(define (serialize-obj out type value)
  (case type
    ['u8 (write-bytes (integer->integer-bytes value 1 #f #f) out)]
    ['u16 (write-bytes (integer->integer-bytes value 2 #f #f) out)]
    ['u32 (write-bytes (integer->integer-bytes value 4 #f #f) out)]
    ['u64 (write-bytes (integer->integer-bytes value 8 #f #f) out)]
    ['i8 (write-bytes (integer->integer-bytes value 1 #t #f) out)]
    ['i16 (write-bytes (integer->integer-bytes value 2 #t #f) out)]
    ['i32 (write-bytes (integer->integer-bytes value 4 #t #f) out)]
    ['i64 (write-bytes (integer->integer-bytes value 8 #t #f) out)]
    ['bool (write-byte (if value 1 0) out)]
    ['string (let ([data (string->bytes/utf-8 value)])
               (serialize-obj out 'u32 (bytes-length data))
               (write-bytes data out))]
    [else
     (case (and (pair? type) (car type))
       ['vec (serialize-obj out 'u32 (vector-length value))
             (for ([item value]) (serialize-obj out (cdr type) item))]

       ; tuple
       [else (for ([item-type type] [item value])
               (serialize-obj out item-type item))])])
  (void))


(define (deserialize-obj in type)
  (case type
    ['u8 (integer-bytes->integer (read-bytes 1 in) #f #f)]
    ['u16 (integer-bytes->integer (read-bytes 2 in) #f #f)]
    ['u32 (integer-bytes->integer (read-bytes 4 in) #f #f)]
    ['u64 (integer-bytes->integer (read-bytes 8 in) #f #f)]
    ['i8 (integer-bytes->integer (read-bytes 1 in) #t #f)]
    ['i16 (integer-bytes->integer (read-bytes 2 in) #t #f)]
    ['i32 (integer-bytes->integer (read-bytes 4 in) #t #f)]
    ['i64 (integer-bytes->integer (read-bytes 8 in) #t #f)]
    ['bool (not (zero? (read-byte in)))]
    ['string (let* ([len (deserialize-obj in 'u32)]
                    [data (read-bytes len in)])
               (bytes->string/utf-8 data))]
    [else
     (case (and (pair? type) (car type))
       ['vec (let ([len (deserialize-obj in 'u32)])
               (for/vector ([_ (build-list len values)]) (deserialize-obj in (cdr type))))]

       ; tuple
       [else (for/list ([item-type type]) (deserialize-obj in item-type))])]))


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

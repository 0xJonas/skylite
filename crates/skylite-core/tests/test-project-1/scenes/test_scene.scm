'(
  ; For the named actors, define two instance of test_actor, with names actor-1 and actor-2
  (actors .
    ((actor-1 . (test_actor 10 10))
     (actor-2 . (test_actor 20 20))))

  ; For the extras, define a third instance of test_actor.
  (extras .
    ((test_actor 30 30)))

  ; Define two parameters, one bool and one u8
  (parameters . ((param1 bool) (param2 u8))))

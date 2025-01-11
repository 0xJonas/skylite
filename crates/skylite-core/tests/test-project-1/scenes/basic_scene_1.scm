'((parameters . ((tag string)))
  (actors .
    ((actor1 . (basic_actor_1 "actor1"))
     (actor2 . (spawn_test_actor "actor2" #t))))
  (extras .
    ((z_order_test_actor "extra1" 2)
     (z_order_test_actor "extra2" -1)
     (z_order_test_actor "extra3" 0))))

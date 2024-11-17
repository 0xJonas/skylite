'((actions .
    ((move ((dx i8 "change in x-coordinate")
            (dy i8 "change in y-coordinate"))
           "Moves the actor by the given amount each update.")
     (idle)

     (set-position ((x i16) (y i16))
                   "Moves the actor to the given position, then idles.")))

  (parameters .
    ((x i16 "initial x-coordinate")
     (y i16 "initial y-coordinate")))

  (initial-action . (idle)))

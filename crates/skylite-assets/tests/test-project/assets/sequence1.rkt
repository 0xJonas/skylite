#lang skylite/asset 'sequence

'([node . node2]
  [subs . ([sub1 . ([return])])]
  [script
   . (-
      [set prop-u16 5]
      [set prop-string "hello"]
      [modify prop-u16 10]
      [modify prop-f32 1.0]
      [modify prop-f64 -1.0]
      [branch prop-bool -]
      [branch (! prop-bool) -]
      [branch (< prop-u16 10) -]
      [branch (> prop-i16 10) -]
      [branch (= prop-f32 10) -]
      [branch (!= prop-f64 10) -]
      [jump -]
      [call sub1]
      [wait 1]
      [run-custom custom-fn]
      [branch-custom custom-cond -])])

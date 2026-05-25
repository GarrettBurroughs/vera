(declare-fun __call_024_generics::identity_I32 () Int)
(assert (not (=> true (and (=> (= __call_024_generics::identity_I32 1) true) (=> (not (= __call_024_generics::identity_I32 1)) true)))))
(check-sat)

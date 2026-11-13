' SUB variables are local unless SHARED; STATIC keeps values between calls.
DIM SHARED Total
X = 5
SUB AddTo (N)
    Total = Total + N
    X = 99
END SUB
SUB Counter
    STATIC Calls
    Calls = Calls + 1
    PRINT "call"; Calls
END SUB
SUB Doubler
    SHARED X
    X = X * 2
END SUB
FUNCTION Fact (BYVAL N)
    IF N <= 1 THEN Fact = 1 ELSE Fact = N * Fact(N - 1)
END FUNCTION
AddTo 10
AddTo 5
PRINT "total"; Total; "x"; X
Counter: Counter: Counter
Doubler
PRINT "x"; X
PRINT "10! ="; Fact(10)
SUB Quit
    PRINT "quitting from a SUB"
    END
END SUB
Quit
PRINT "not reached"

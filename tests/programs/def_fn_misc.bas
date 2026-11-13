DEF FNSquare (X) = X * X
DEF FNGreet$ (N$) = "Hi " + N$
PRINT FNSquare(7), FNGreet$("Bob")
CONST Pi = 3.14159
CONST Greeting$ = "hello"
PRINT Pi * 2, Greeting$
DEFINT A-C
Apples = 2.6
PRINT Apples
FUNCTION Twice (N)
    Twice = N * 2
END FUNCTION
PRINT Twice(Twice(ABS(-3)))
RANDOMIZE 7
R = RND
PRINT R >= 0 AND R < 1, RND(0) = R

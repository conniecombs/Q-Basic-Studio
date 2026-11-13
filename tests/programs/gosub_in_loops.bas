' GOSUB from inside nested blocks returns to the statement after it.
FOR I = 1 TO 3
    IF I MOD 2 = 1 THEN
        GOSUB Show
    END IF
NEXT I
N = 2
DO WHILE N > 0
    ON N GOSUB One, Two
    N = N - 1
LOOP
PRINT "done"
END

Show:
PRINT "odd"; I
RETURN
One: PRINT "one": RETURN
Two: PRINT "two": RETURN

FOR I = 1 TO 3: PRINT I;: NEXT I: PRINT
I = 0: WHILE I < 3: I = I + 1: WEND: PRINT I
DO: I = I - 1: LOOP UNTIL I = 0: PRINT I
X = 2
SELECT CASE X
    CASE 1: PRINT "one"
    CASE 2, 3: PRINT "two or three"
    CASE ELSE: PRINT "other"
END SELECT
FOR I = 1 TO 2
FOR J = 1 TO 2
PRINT I; J
NEXT J, I
IF X = 2 THEN PRINT "yes" ELSE PRINT "no"
IF X > 5 THEN PRINT "big" ELSE IF X > 1 THEN PRINT "medium" ELSE PRINT "small"
start: PRINT "label then statement": IF X < 4 THEN X = X + 1: GOTO start

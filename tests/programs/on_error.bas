ON ERROR GOTO Handler
D = 0
PRINT 10 / D
ERROR 53
OPEN "missing-file.txt" FOR INPUT AS #1
PRINT "end of main"
END

Handler:
PRINT "error"; ERR
SELECT CASE ERR
    CASE 11
        D = 5
        RESUME
    CASE 53
        RESUME NEXT
END SELECT
RESUME NEXT

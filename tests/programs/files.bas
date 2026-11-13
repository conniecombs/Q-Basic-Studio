F = FREEFILE
OPEN "data.txt" FOR OUTPUT AS #F
PRINT #F, "first line"
WRITE #F, "a, b", 2
PRINT #F, 3; ","; 4
CLOSE #F
OPEN "data.txt" FOR APPEND AS #1
PRINT #1, "appended"
CLOSE #1
OPEN "data.txt" FOR INPUT AS #1
LINE INPUT #1, L$
PRINT "line: "; L$
INPUT #1, A$, B
PRINT "fields: ["; A$; "]"; B
DO UNTIL EOF(1)
    LINE INPUT #1, L$
    PRINT "rest: "; L$
LOOP
CLOSE #1

TYPE Rec
    Id AS INTEGER
    Label AS STRING * 4
END TYPE
DIM R AS Rec
OPEN "recs.dat" FOR RANDOM AS #2 LEN = LEN(R)
FOR I = 1 TO 3
    R.Id = I * 10: R.Label = "r" + LTRIM$(STR$(I))
    PUT #2, I, R
NEXT I
GET #2, 2, R
PRINT R.Id; "["; R.Label; "]"; LOF(2)
CLOSE
KILL "recs.dat"
KILL "data.txt"
PRINT "cleaned up"

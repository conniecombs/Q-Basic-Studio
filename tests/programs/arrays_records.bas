TYPE Item
    Name AS STRING * 6
    Qty AS INTEGER
END TYPE
DIM Stock(1 TO 3) AS Item
Stock(1).Name = "apple": Stock(1).Qty = 3
Stock(2).Name = "pear": Stock(2).Qty = 7
FOR I = LBOUND(Stock) TO UBOUND(Stock)
    PRINT "["; Stock(I).Name; "]"; Stock(I).Qty
NEXT I
DIM Grid(-1 TO 1, 2)
Grid(-1, 2) = 9
PRINT Grid(-1, 2), LBOUND(Grid, 1), UBOUND(Grid, 2)
REDIM PRESERVE Grid(-1 TO 1, 3)
PRINT Grid(-1, 2), UBOUND(Grid, 2)
Auto(10) = 4
PRINT Auto(10), UBOUND(Auto)
SUB Fill (A())
    FOR K = LBOUND(A) TO UBOUND(A): A(K) = K * K: NEXT
END SUB
DIM Sq(4)
Fill Sq()
PRINT Sq(0), Sq(2), Sq(4)
ERASE Sq

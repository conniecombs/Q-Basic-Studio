S$ = "Hello, World"
PRINT LEFT$(S$, 5), RIGHT$(S$, 5), MID$(S$, 8), MID$(S$, 8, 3)
PRINT LEN(S$), INSTR(S$, "o"), INSTR(6, S$, "o"), INSTR(S$, "z")
MID$(S$, 1, 5) = "Jelly"
PRINT S$
PRINT UCASE$("abc"), LCASE$("DEF"), "[", LTRIM$("  x  "), "][", RTRIM$("  x  "), "]"
PRINT STRING$(3, "*"), STRING$(2, 65), SPACE$(2)
PRINT ASC("A"), CHR$(66), ASC(CHR$(201)), CHR$(196), CHR$(205)
A$ = "b": B$ = "a"
IF A$ > B$ THEN PRINT "b sorts after a"
SWAP A$, B$
PRINT A$, B$

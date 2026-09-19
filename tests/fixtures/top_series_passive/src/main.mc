// U105 minimal case: one two-pin passive in *series* between two top-level
// blocks, plus its control (the same two blocks joined directly).
//
// The series passive is the only thing joining A.OP to B.IP, so dropping it
// leaves both pins dangling on the drawing while the netlist stays connected —
// the reading `requirements-design.md` D1/E1 forbid.

module BLKA {
    io OP
}

module BLKB {
    io IP
}

// Under test: the passive carries the connection.
module series {
    BLKA A
    BLKB B
    A.OP -> RES(10k) -> B.IP
}

// Control: no passive, so C5 never touches this layer. It shows what "the
// connection is drawn" looks like on the same two blocks.
module direct {
    BLKA A
    BLKB B
    A.OP -> B.IP
}

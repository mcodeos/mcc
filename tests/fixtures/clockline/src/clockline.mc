# ---------------------------------------------------------------------------------------------
#  Copyright (c) MCODE. All rights reserved.
# ---------------------------------------------------------------------------------------------

# Purpose-built fixture for the clock-intent profile row (U112 ③):
# three adoption shapes that claim, plus two that must never claim.

# The pairing family — mutual peers with `exclusive = true`, no direction
# words anywhere (the XTAL face's shape; the E4122 / 6054 anchor).
interface XTF(role)
{
    topology = "point to point"
    pins = [
        [1,2] = [X1, X2]
    ]
    role Osc
    {
        peer = Res
        exclusive = true
    }
    role Res
    {
        peer = Osc
        exclusive = true
    }
}

# The unidirectional family — a source role whose pins all declare `out`
# beside a sink role whose pins all declare `in` (the 6060 anchor shape).
interface CKF(role)
{
    topology = "point to point"
    mode = ["unidirectional"]
    pins = [
        1 = CK
    ]
    role Src
    {
        pins = [
            out 1 = CK
        ]
        peer = Snk
    }
    role Snk
    {
        pins = [
            in 1 = CK
        ]
        peer = Src
    }
}

# The mixed family — `out` TX beside `in` RX in one role: no uniform
# direction shape, no exclusive pairing, so no family may claim its nets.
interface MBF(role)
{
    pins = [
        [1,2] = [TX, RX]
    ]
    role DCE
    {
        pins = [
            out 1 = TX
            in 2 = RX
        ]
    }
}

component CRY
{
    pins = [
        [1,2] = XT::XTF(Res)
    ]
}

component MCU
{
    pins = [
        [3,4] = XTAL::XTF(Osc)
    ]
}

component OSC
{
    pins = [
        1 = DRV::CKF(Src)
    ]
}

component SINK
{
    pins = [
        1 = RCV::CKF(Snk)
    ]
}

component DCE1
{
    pins = [
        [1,2] = LINK::MBF(DCE)
    ]
}

# The roleless adoption — the family named with an empty role: no role
# declaration is carried, so no shape exists for a family to claim.
component NAD
{
    pins = [
        [1,2] = Y::XTF()
    ]
}

module main
{
    MCU m1
    CRY c1
    OSC o1
    SINK s1
    DCE1 d1
    NAD n1

    m1.XTAL.X1 -> c1.XT.X1
    m1.XTAL.X2 -> c1.XT.X2
    o1.DRV.CK -> s1.RCV.CK
    d1.LINK.TX -> n1.Y.X1
}

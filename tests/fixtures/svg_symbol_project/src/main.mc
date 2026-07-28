component USB.MINI_B
{
    pins = [
        1 = VBUS
        2 = D_NEG
    ]
}

module main
{
    USB.MINI_B J1
    USB.MINI_B J2

    J1.VBUS -> J2.VBUS
    J1.D_NEG -> J2.D_NEG
}

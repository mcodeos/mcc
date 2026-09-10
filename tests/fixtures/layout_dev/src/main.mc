# ---------------------------------------------------------------------------------------------
#  Layout-first policy fixture: a module sub-layer holding a component that declares
#  `layout=[...]`. The device pipeline must honor it — listed pins (connected or not)
#  take the declared edge in the declared counterclockwise order.
# ---------------------------------------------------------------------------------------------

component CONN.USB_MINI_B
{
    partno = "HUM011D-5-S"

    pins = [
        1 = VBUS
        5 = GND
        2 = D\+
        3 = D\-
        4 = ID
        [6,7] = GND
        8 = SHIELD3
        9 = SHIELD4
    ]

    layout = [
        right  = [4, 3, 2, 5, 1]
        bottom = [6:9]
    ]
}

module PORT_USB()
{
    io vin{POWER_SYS, GND}::DC(5V)

    CONN.USB_MINI_B sock
    TP1::TP()

    ((sock.VBUS -> USB_VBUS) + TP1) -> RES(0R) -> vin.POWER_SYS
    (sock.5 + sock.6 + sock.7 + sock.SHIELD3 + sock.SHIELD4) -> vin.GND
}

module main
{
    PORT_USB USB
}

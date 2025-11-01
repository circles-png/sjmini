# sjmini

find the time to the next bell time with only a raspberry pi pico 2 w and a 4 digit 7 segment display

## software side

written in rust using the open source subjective crate

on weekends, device does nothing, shows `End `

on weekdays, device shows time to next subject in `mm:ss` format, shows `hh:mm` format when no more bells are left for the day

finds the time through ntp to a precision of basically ±0 seconds (request in `src/ntp_tx` specifies a precision of `2^-18` seconds)

place your bell times in `./timetable.subjective` in the `.subjective` format as exported from the web app to embed it at compile time

run `./flash_cyw43_fw` to flash the cyw43 firmware to the wifi chip with `probe-rs`

use `WIFI_SSID=... WIFI_PASSWORD=... cargo run` to compile and deploy it with `probe-rs`

## hardware side

- raspberry pi pico 2 w
- 4 digit 7 segment display (common cathode)
- eight 220 ohm resistors for segment current limiting, one for each anode connection (seven segments plus decimal point/colon)
- many wires

connect them according to your display's datasheet

cathodes are pulled high and low one at a time and are switched every millisecond

anodes are switched between hi-z and low on the other side

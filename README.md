# sjmini

find the time to the next bell time with only a raspberry pi pico 2 w and a 4 digit 7 segment display

## software side

written in rust using the open source subjective crate

on weekends, shows current time in `hh:mm` format

on weekdays, device shows time to next subject in `mm:ss` format; when no more bells are left for the day, device does nothing, shows `End `

finds the time through ntp to a precision of basically ±0 seconds (request in `src/ntp_tx` specifies a precision of `2^-18` seconds)

place your bell times in `./timetable.subjective` in the `.subjective` format as exported from the web app to embed it at compile time

run `./flash_cyw43_fw` to flash the cyw43 firmware to the wifi chip with `probe-rs`

use `WIFI_SSID=... WIFI_PASSWORD=... cargo run` to compile and deploy it with `probe-rs`

## hardware side

- raspberry pi pico 2 w
- 4 digit 7 segment display (common cathode)
- four 220 ohm resistors for segment current limiting, one for each cathode connection (four commons)
- many wires

connect them according to your display's datasheet, then reassign pins in the `main` routine as necessary

anodes are pulled high and low one at a time to multiplex the display

cathodes are switched between hi-z and pulled low on the other side and are switched every millisecond

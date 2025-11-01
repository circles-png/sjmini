#![allow(clippy::future_not_send)]
#![no_std]
#![no_main]

use core::{
    net::{Ipv4Addr, SocketAddrV4},
    slice::from_raw_parts,
    time::Duration,
};

use chrono::{DateTime, Datelike, NaiveTime, TimeDelta, Timelike};
use cyw43::{JoinOptions, PowerManagementMode};
use cyw43_pio::{PioSpi, RM2_CLOCK_DIVIDER};
use defmt::info;
use embassy_executor::{Spawner, task};
use embassy_futures::yield_now;
use embassy_net::{
    DhcpConfig, StackResources,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_rp::{
    Peri, bind_interrupts,
    clocks::RoscRng,
    config,
    gpio::{Flex, Level, Output, Pull},
    peripherals::{DMA_CH0, PIN_23, PIN_24, PIN_25, PIN_29, PIO0},
    pio::{InterruptHandler, Pio},
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex, once_lock::OnceLock};
use embassy_time::Instant;
use fixed::{FixedU64, types::extra::U32};
use static_cell::StaticCell;
use subjective::{
    FindBellError, Subjective,
    school::{School, Week, bells::BellTime},
};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[task]
async fn cyw43_runner_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) {
    runner.run().await;
}

#[task]
async fn net_runner_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) {
    runner.run().await;
}

#[task]
async fn get_start_time_task(
    spawner: Spawner,
    pwr: Peri<'static, PIN_23>,
    pio0: Peri<'static, PIO0>,
    cs: Peri<'static, PIN_25>,
    mosi: Peri<'static, PIN_24>,
    clk: Peri<'static, PIN_29>,
    dma_ch0: Peri<'static, DMA_CH0>,
) {
    static STATE: StaticCell<cyw43::State> = StaticCell::new();

    info!("Getting start time from NTP server...");
    let (net_device, mut control, runner) = cyw43::new(
        STATE.init(cyw43::State::new()),
        Output::new(pwr, Level::Low),
        {
            let mut pio = Pio::new(pio0, Irqs);
            PioSpi::new(
                &mut pio.common,
                pio.sm0,
                RM2_CLOCK_DIVIDER,
                pio.irq0,
                Output::new(cs, Level::High),
                mosi,
                clk,
                dma_ch0,
            )
        },
        unsafe { from_raw_parts(0x1010_0000 as *const u8, 230_321) },
    )
    .await;
    spawner.spawn(cyw43_runner_task(runner)).unwrap();
    control
        .init(unsafe { from_raw_parts(0x1014_0000 as *const u8, 4752) })
        .await;
    control
        .set_power_management(PowerManagementMode::PowerSave)
        .await;
    info!("Connecting to WiFi...");
    *PROGRESS.lock().await = 1;
    while control
        .join(env!("WIFI_SSID"), {
            let mut options = JoinOptions::default();
            options.passphrase = env!("WIFI_PASSWORD").as_bytes();
            options
        })
        .await
        .is_err()
    {}

    let (stack, runner) = {
        static STACK_RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();
        embassy_net::new(
            net_device,
            embassy_net::Config::dhcpv4(DhcpConfig::default()),
            STACK_RESOURCES.init(StackResources::new()),
            RoscRng.next_u64(),
        )
    };
    spawner.spawn(net_runner_task(runner)).unwrap();
    *PROGRESS.lock().await = 2;
    stack.wait_config_up().await;

    let mut socket = {
        static RX_META: StaticCell<[PacketMetadata; 4]> = StaticCell::new();
        static RX_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();
        static TX_META: StaticCell<[PacketMetadata; 4]> = StaticCell::new();
        static TX_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();
        UdpSocket::new(
            stack,
            RX_META.init([PacketMetadata::EMPTY; 4]),
            RX_BUFFER.init([0; 256]),
            TX_META.init([PacketMetadata::EMPTY; 4]),
            TX_BUFFER.init([0; 256]),
        )
    };
    socket.bind(0).unwrap();
    *PROGRESS.lock().await = 3;
    socket.wait_send_ready().await;
    socket
        .send_to(
            include_bytes!("ntp_tx"),
            SocketAddrV4::new(Ipv4Addr::new(162, 159, 200, 1), 123),
        )
        .await
        .unwrap();
    *PROGRESS.lock().await = 4;
    socket.wait_recv_ready().await;
    let mut buf = [0u8; 256];
    socket.recv_from(&mut buf).await.unwrap();
    let bytes = buf[40..48].try_into().unwrap();
    let date_time = DateTime::parse_from_rfc2822("Mon, 1 Jan 1900 00:00:00 +0000").unwrap()
        + TimeDelta::from_std(Duration::from_secs_f64(
            FixedU64::<U32>::from_be_bytes(bytes).to_num::<f64>(),
        ))
        .unwrap()
        + TimeDelta::hours(11)
        - TimeDelta::milliseconds(Instant::now().as_millis().cast_signed());
    info!(
        "Date Time: {} {} {} {} {} {}",
        date_time.day(),
        date_time.month(),
        date_time.year(),
        date_time.hour(),
        date_time.minute(),
        date_time.second()
    );
    START.init(date_time).unwrap();
}

static START: OnceLock<DateTime<chrono::FixedOffset>> = OnceLock::new();
static PROGRESS: Mutex<ThreadModeRawMutex, u8> = Mutex::new(0);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(config::Config::default());

    let digit_commons = [
        Output::new(p.PIN_19, Level::Low),
        Output::new(p.PIN_18, Level::Low),
        Output::new(p.PIN_17, Level::Low),
        Output::new(p.PIN_16, Level::Low),
    ];
    let a = Flex::new(p.PIN_13);
    let b = Flex::new(p.PIN_15);
    let c = Flex::new(p.PIN_9);
    let d = Flex::new(p.PIN_11);
    let e = Flex::new(p.PIN_12);
    let f = Flex::new(p.PIN_14);
    let g = Flex::new(p.PIN_10);
    let point = Flex::new(p.PIN_8);
    let subjective = include!(concat!(env!("OUT_DIR"), "/timetable.rs"));
    spawner
        .spawn(get_start_time_task(
            spawner, p.PIN_23, p.PIO0, p.PIN_25, p.PIN_24, p.PIN_29, p.DMA_CH0,
        ))
        .unwrap();
    App {
        segments: Segments {
            a,
            b,
            c,
            d,
            e,
            f,
            g,
            point,
        },
        commons: digit_commons,
        subjective,
    }
    .run()
    .await;
}

struct App {
    segments: Segments,
    commons: [Output<'static>; 4],
    subjective: Subjective<'static, 'static, 'static>,
}

impl App {
    async fn run(&mut self) {
        loop {
            'block: {
                let colon = Instant::now().as_millis() % 1000 < 500;
                let Some(now) = START.try_get().map(|start| {
                    (*start + TimeDelta::milliseconds(Instant::now().as_millis().cast_signed()))
                        .naive_utc()
                }) else {
                    let mut digits = [0b0000_0000; 4];
                    {
                        let progress = PROGRESS.lock().await;
                        for i in 0..*progress {
                            digits[i as usize] = 0b0000_0010;
                        }
                    }
                    self.render_state(&State { colon, digits });
                    break 'block;
                };

                match self.subjective.find_first_after(now, 0) {
                    Ok(next) => {
                        let delta = next.time - now.time();
                        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                        self.render_state(&State {
                            colon,
                            digits: [
                                State::DIGITS[(delta.num_minutes() / 10) as usize],
                                State::DIGITS[(delta.num_minutes() % 10) as usize],
                                State::DIGITS[(delta.num_seconds() % 60 / 10) as usize],
                                State::DIGITS[(delta.num_seconds() % 60 % 10) as usize],
                            ],
                        });
                    }
                    Err(FindBellError::WeekdayOutOfRange(_)) => {
                        self.render_state(&State {
                            colon,
                            digits: [
                                State::DIGITS[(now.hour() / 10) as usize],
                                State::DIGITS[(now.hour() % 10) as usize],
                                State::DIGITS[(now.minute() / 10) as usize],
                                State::DIGITS[(now.minute() % 10) as usize],
                            ],
                        });
                    }
                    Err(FindBellError::NoBellFound) => {
                        self.render_state(&State {
                            colon,
                            digits: [0b1001_1110, 0b0010_1010, 0b0111_1010, 0b0000_0000],
                        });
                    }
                }
            }
            yield_now().await;
        }
    }

    fn render_state(&mut self, state: &State) {
        const DIVISOR: u64 = 1;
        let digit = ((Instant::now().as_millis() / DIVISOR) % 4) as usize;
        for (index, common) in self.commons.iter_mut().enumerate() {
            common.set_level((index == digit).into());
        }
        for (index, segment) in self.segments.as_array_mut().iter_mut().enumerate() {
            let bit = if digit == 1 && index == 0 {
                state.colon
            } else {
                (state.digits[digit] >> (index)) & 1 == 1
            };
            if bit {
                segment.set_as_output();
                segment.set_low();
            } else {
                segment.set_as_input();
                segment.set_pull(Pull::None);
            }
        }
    }
}

struct Segments {
    a: Flex<'static>,
    b: Flex<'static>,
    c: Flex<'static>,
    d: Flex<'static>,
    e: Flex<'static>,
    f: Flex<'static>,
    g: Flex<'static>,
    point: Flex<'static>,
}

impl Segments {
    const fn as_array_mut(&mut self) -> [&mut Flex<'static>; 8] {
        [
            &mut self.a,
            &mut self.b,
            &mut self.c,
            &mut self.d,
            &mut self.e,
            &mut self.f,
            &mut self.g,
            &mut self.point,
        ]
    }
}

struct State {
    colon: bool,
    digits: [u8; 4],
}

impl State {
    const DIGITS: [u8; 10] = [
        0b1111_1100,
        0b0110_0000,
        0b1101_1010,
        0b1111_0010,
        0b0110_0110,
        0b1011_0110,
        0b1011_1110,
        0b1110_0000,
        0b1111_1110,
        0b1111_0110,
    ];
}

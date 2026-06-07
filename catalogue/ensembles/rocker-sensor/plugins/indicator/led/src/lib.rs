//! # r2-plugin-indicator-led
//!
//! R2 AOT plugin: status-LED driver. Maps an FSM state (wire-compatible
//! with the r2-workshop dashboard's `ledClassFor()` / firmware
//! `LedState`) to a representative RGB colour and writes it through the
//! generic [`LedSink`] trait. Provides
//! `ai.reality2.workshop.cap.status-led`.
//!
//! **Reference implementation:** the host-firmware module
//! `r2-workshop/firmware/esp32-s3/devkitc/src/led.rs`. This crate lifts
//! that module's *portable, glanceable contract* — the state byte map and
//! the per-state colour — off the WS2812/RMT animator thread it ran in.
//!
//! ## What is (and isn't) in v0.1
//!
//! The reference renders **animated** colours (pulse / heartbeat / strobe
//! / tick) from a 30 Hz thread driving a WS2812 over RMT. Those envelopes
//! need a periodic render tick and float transcendentals (`sin`/`exp`) —
//! both firmware-loop concerns, not things a command-driven R2 plugin
//! owns. So v0.1 writes each state's **base (solid) colour**; the
//! animation envelope is a deferred firmware-render enhancement. A solid
//! colour still conveys the state at a glance — only the pulse nuance
//! (e.g. solid-purple `Calibrating` vs slow-pulse-purple
//! `StreamingDegradedSim`) is lost until the animator lands.
//!
//! ## Sink abstraction
//!
//! `LedSink::write_rgb` is the one platform hook. The firmware-render step
//! wraps a WS2812 RMT driver (RGB carriers: DevKitC) or maps the colour's
//! luma onto a mono LEDC PWM channel (single-colour carriers: XIAO,
//! dfr1117). Keeping the plugin behind this trait is why it stays no_std.
//!
//! ## Command opcodes (mirrors `plugin.toml [commands]`)
//!
//! | Byte | Name      | Input                   | Output |
//! |------|-----------|-------------------------|--------|
//! | 0x01 | init      | `[]`                    | `[]`   |
//! | 0x02 | set_state | `[state: u8]`           | `[r,g,b]` (the colour written) |
//! | 0x03 | set_color | `[r: u8, g: u8, b: u8]` | `[r,g,b]` |
//! | 0x04 | off       | `[]`                    | `[]`   |

#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![deny(missing_docs)]

#[cfg(all(feature = "aot", feature = "nif"))]
compile_error!("features `aot` and `nif` are mutually exclusive");

use r2_engine::plugin::{
    Plugin, PluginCommand, PluginError, PluginId, PluginResponse, PluginResult,
};

/// A 24-bit RGB colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Rgb {
    /// Construct an RGB colour.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
    /// Black / off.
    pub const OFF: Rgb = Rgb::new(0, 0, 0);
}

/// The one platform hook: write a colour to the physical LED.
///
/// Firmware supplies an impl wrapping a WS2812 RMT driver (RGB) or a mono
/// LEDC PWM channel (single-colour, driven by the colour's luma). `Err(())`
/// signals a transient write failure.
pub trait LedSink {
    /// Write `colour` to the LED.
    fn write_rgb(&mut self, colour: Rgb) -> Result<(), ()>;
}

/// FSM state — byte values wire-compatible with the r2-workshop firmware
/// `LedState` and the dashboard's virtual LED.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LedState {
    /// Cold boot — white flash.
    Boot = 0,
    /// BLE advertising — blue.
    Advertising = 1,
    /// BLE connected — cyan.
    BleConnected = 2,
    /// Joining WiFi — cyan (rendered as BleConnected in the reference).
    WifiConnecting = 3,
    /// Streaming live samples — green.
    StreamingLive = 4,
    /// Streaming while catching up a backlog — yellow.
    StreamingCatchup = 5,
    /// Calibrating — purple (solid).
    Calibrating = 6,
    /// Low battery overlay — orange.
    LowBattery = 7,
    /// OTA in progress — white.
    Ota = 8,
    /// Error — red.
    Error = 9,
    /// Streaming with simulated data (sensor init failed) — purple.
    StreamingDegradedSim = 10,
    /// Actively recording to a capture file — green.
    Recording = 11,
    /// Operator "identify" overlay — solid white.
    Identify = 12,
}

impl LedState {
    /// Decode a state byte. Unknown values fall back to [`LedState::Boot`]
    /// (matching the reference firmware's saturating decode).
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Boot,
            1 => Self::Advertising,
            2 => Self::BleConnected,
            3 => Self::WifiConnecting,
            4 => Self::StreamingLive,
            5 => Self::StreamingCatchup,
            6 => Self::Calibrating,
            7 => Self::LowBattery,
            8 => Self::Ota,
            9 => Self::Error,
            10 => Self::StreamingDegradedSim,
            11 => Self::Recording,
            12 => Self::Identify,
            _ => Self::Boot,
        }
    }

    /// The representative solid colour for this state — the reference
    /// firmware's animated colour with its pulse/heartbeat envelope
    /// removed (see module docs on the v0.1 animation deferral).
    pub fn base_colour(self) -> Rgb {
        match self {
            Self::Boot => Rgb::new(0x40, 0x40, 0x40),
            Self::Advertising => Rgb::new(0x00, 0x00, 0xFF),
            Self::BleConnected | Self::WifiConnecting => Rgb::new(0x00, 0xC0, 0xC0),
            Self::StreamingLive => Rgb::new(0x00, 0xC0, 0x20),
            Self::StreamingCatchup => Rgb::new(0xFF, 0xCC, 0x00),
            Self::Calibrating | Self::StreamingDegradedSim => Rgb::new(0x80, 0x00, 0xC0),
            Self::LowBattery => Rgb::new(0xFF, 0x80, 0x00),
            Self::Ota => Rgb::new(0x40, 0x40, 0x40),
            Self::Error => Rgb::new(0xFF, 0x00, 0x00),
            Self::Recording => Rgb::new(0x00, 0xE0, 0x30),
            Self::Identify => Rgb::new(0xFF, 0xFF, 0xFF),
        }
    }
}

/// Command opcode: initialise (LED to off).
pub const CMD_INIT: PluginCommand = 0x01;
/// Command opcode: set the LED to a named FSM state's colour.
pub const CMD_SET_STATE: PluginCommand = 0x02;
/// Command opcode: set a raw RGB colour (advanced override).
pub const CMD_SET_COLOR: PluginCommand = 0x03;
/// Command opcode: turn the LED off.
pub const CMD_OFF: PluginCommand = 0x04;

/// Error code: input byte length did not match the command's layout.
pub const ERR_BAD_LENGTH: u8 = 0x01;
/// Error code: the LED sink write failed.
pub const ERR_SINK: u8 = 0x02;
/// Error code: command byte not recognised.
pub const ERR_UNKNOWN_COMMAND: u8 = 0xFE;

/// Status-LED plugin, generic over an [`LedSink`].
pub struct Led<S: LedSink> {
    sink: S,
    id: PluginId,
}

impl<S: LedSink> Led<S> {
    /// Construct a plugin bound to `id`.
    pub const fn new(sink: S, id: PluginId) -> Self {
        Self { sink, id }
    }

    fn write(&mut self, colour: Rgb) -> Result<(), PluginError> {
        self.sink
            .write_rgb(colour)
            .map_err(|_| PluginError::new(ERR_SINK, "led: sink write failed"))
    }
}

impl<S: LedSink> Plugin for Led<S> {
    fn execute(&mut self, command: PluginCommand, data: &[u8]) -> PluginResult {
        match command {
            CMD_INIT | CMD_OFF => match self.write(Rgb::OFF) {
                Ok(()) => PluginResult::Ok(PluginResponse::empty()),
                Err(e) => PluginResult::Error(e),
            },
            CMD_SET_STATE => {
                if data.len() != 1 {
                    return PluginResult::Error(PluginError::new(
                        ERR_BAD_LENGTH,
                        "led: set_state expects exactly 1 state byte",
                    ));
                }
                let colour = LedState::from_u8(data[0]).base_colour();
                match self.write(colour) {
                    Ok(()) => PluginResult::Ok(PluginResponse::with_data(&[colour.r, colour.g, colour.b])),
                    Err(e) => PluginResult::Error(e),
                }
            }
            CMD_SET_COLOR => {
                if data.len() != 3 {
                    return PluginResult::Error(PluginError::new(
                        ERR_BAD_LENGTH,
                        "led: set_color expects exactly 3 bytes [r,g,b]",
                    ));
                }
                let colour = Rgb::new(data[0], data[1], data[2]);
                match self.write(colour) {
                    Ok(()) => PluginResult::Ok(PluginResponse::with_data(&[colour.r, colour.g, colour.b])),
                    Err(e) => PluginResult::Error(e),
                }
            }
            _ => PluginResult::Error(PluginError::new(
                ERR_UNKNOWN_COMMAND,
                "led: unknown command byte",
            )),
        }
    }

    fn name(&self) -> &str {
        "indicator/led"
    }

    fn id(&self) -> PluginId {
        self.id
    }

    fn init(&mut self) -> PluginResult {
        match self.write(Rgb::OFF) {
            Ok(()) => PluginResult::Ok(PluginResponse::empty()),
            Err(e) => PluginResult::Error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records the last colour written.
    struct SpySink {
        last: Option<Rgb>,
        fail: bool,
    }
    impl SpySink {
        fn new() -> Self {
            Self { last: None, fail: false }
        }
        fn failing() -> Self {
            Self { last: None, fail: true }
        }
    }
    impl LedSink for SpySink {
        fn write_rgb(&mut self, colour: Rgb) -> Result<(), ()> {
            if self.fail {
                return Err(());
            }
            self.last = Some(colour);
            Ok(())
        }
    }

    #[test]
    fn state_byte_round_trips() {
        for v in 0u8..=12 {
            assert_eq!(LedState::from_u8(v) as u8, v);
        }
        // Out-of-range saturates to Boot.
        assert_eq!(LedState::from_u8(200) as u8, LedState::Boot as u8);
    }

    #[test]
    fn base_colours_match_reference() {
        assert_eq!(LedState::Advertising.base_colour(), Rgb::new(0, 0, 0xFF));
        assert_eq!(LedState::StreamingLive.base_colour(), Rgb::new(0, 0xC0, 0x20));
        assert_eq!(LedState::Error.base_colour(), Rgb::new(0xFF, 0, 0));
        assert_eq!(LedState::Identify.base_colour(), Rgb::new(0xFF, 0xFF, 0xFF));
        // Calibrating and DegradedSim share the same base purple (the
        // reference distinguishes them only by animation, deferred here).
        assert_eq!(
            LedState::Calibrating.base_colour(),
            LedState::StreamingDegradedSim.base_colour()
        );
    }

    #[test]
    fn init_and_off_write_black() {
        let mut p = Led::new(SpySink::new(), 7);
        assert!(matches!(p.execute(CMD_INIT, &[]), PluginResult::Ok(_)));
        assert_eq!(p.sink.last, Some(Rgb::OFF));
        // Light it, then OFF returns to black.
        let _ = p.execute(CMD_SET_COLOR, &[1, 2, 3]);
        assert!(matches!(p.execute(CMD_OFF, &[]), PluginResult::Ok(_)));
        assert_eq!(p.sink.last, Some(Rgb::OFF));
    }

    #[test]
    fn set_state_writes_mapped_colour_and_returns_it() {
        let mut p = Led::new(SpySink::new(), 7);
        let PluginResult::Ok(resp) = p.execute(CMD_SET_STATE, &[LedState::Error as u8]) else {
            panic!("expected Ok");
        };
        assert_eq!(resp.as_slice(), &[0xFF, 0x00, 0x00]);
        assert_eq!(p.sink.last, Some(Rgb::new(0xFF, 0, 0)));
    }

    #[test]
    fn set_state_bad_length_errors() {
        let mut p = Led::new(SpySink::new(), 7);
        let PluginResult::Error(e) = p.execute(CMD_SET_STATE, &[]) else { panic!() };
        assert_eq!(e.code, ERR_BAD_LENGTH);
    }

    #[test]
    fn set_color_writes_raw_rgb() {
        let mut p = Led::new(SpySink::new(), 7);
        let PluginResult::Ok(resp) = p.execute(CMD_SET_COLOR, &[0x12, 0x34, 0x56]) else {
            panic!("expected Ok");
        };
        assert_eq!(resp.as_slice(), &[0x12, 0x34, 0x56]);
        assert_eq!(p.sink.last, Some(Rgb::new(0x12, 0x34, 0x56)));
    }

    #[test]
    fn set_color_bad_length_errors() {
        let mut p = Led::new(SpySink::new(), 7);
        let PluginResult::Error(e) = p.execute(CMD_SET_COLOR, &[0x12, 0x34]) else { panic!() };
        assert_eq!(e.code, ERR_BAD_LENGTH);
    }

    #[test]
    fn sink_failure_surfaces_err_sink() {
        let mut p = Led::new(SpySink::failing(), 7);
        let PluginResult::Error(e) = p.execute(CMD_SET_STATE, &[LedState::Boot as u8]) else {
            panic!()
        };
        assert_eq!(e.code, ERR_SINK);
    }

    #[test]
    fn unknown_command_errors() {
        let mut p = Led::new(SpySink::new(), 7);
        let PluginResult::Error(e) = p.execute(0xAA, &[]) else { panic!() };
        assert_eq!(e.code, ERR_UNKNOWN_COMMAND);
    }
}

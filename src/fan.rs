use anyhow::{Context, Result, anyhow};
use libc::c_char;
use mach2::kern_return::kern_return_t as IOReturn;
use mach2::kern_return::KERN_SUCCESS;
use mach2::port::mach_port_t;
use mach2::traps::mach_task_self;
use std::thread;
use std::time::Duration;

type IOObject = mach_port_t;
type IOService = mach_port_t;
type IOConnect = mach_port_t;
type CFDictionaryRef = *const libc::c_void;

const KERNEL_INDEX_SMC: u32 = 2;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CFDictionaryRef;
    fn IOServiceGetMatchingServices(
        main_port: mach_port_t,
        matching: CFDictionaryRef,
        existing: *mut IOObject,
    ) -> IOReturn;
    fn IOIteratorNext(iterator: IOObject) -> IOObject;
    fn IOObjectRelease(object: IOObject) -> IOReturn;
    fn IOServiceOpen(
        service: IOService,
        owning_task: mach_port_t,
        connect_type: u32,
        connect: *mut IOConnect,
    ) -> IOReturn;
    fn IOServiceClose(connect: IOConnect) -> IOReturn;
    fn IOConnectCallStructMethod(
        connection: IOConnect,
        selector: u32,
        input: *const libc::c_void,
        input_size: usize,
        output: *mut libc::c_void,
        output_size: *mut usize,
    ) -> IOReturn;
}

const SMC_DATA_SIZE: usize = 32;
const SMC_CMD_READ_KEYINFO: u8 = 9;
const SMC_CMD_READ_BYTES: u8 = 5;
const SMC_CMD_WRITE_BYTES: u8 = 6;
const SMC_ERR_NOT_WRITABLE: u8 = 0x82;

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
struct SMCKeyDataVers {
    major: u8,
    minor: u8,
    build: u8,
    reserved: u8,
    release: u16,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
struct SMCKeyDataPLimitData {
    version: u16,
    length: u16,
    cpu_plimit: u32,
    gpu_plimit: u32,
    mem_plimit: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
struct SMCKeyDataKeyInfo {
    data_size: u32,
    data_type: u32,
    data_attributes: u8,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
union SMCBytes {
    bytes: [u8; SMC_DATA_SIZE],
}
impl Default for SMCBytes {
    fn default() -> Self { SMCBytes { bytes: [0; SMC_DATA_SIZE] } }
}

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
struct SMCKeyData {
    key: u32,
    vers: SMCKeyDataVers,
    p_limit_data: SMCKeyDataPLimitData,
    key_info: SMCKeyDataKeyInfo,
    result: u8,
    status: u8,
    data8: u8,
    data32: u32,
    bytes: SMCBytes,
}

type SMCParamStruct = SMCKeyData;

const _: () = {
    assert!(std::mem::size_of::<SMCKeyDataVers>() == 6);
    assert!(std::mem::size_of::<SMCKeyDataPLimitData>() == 16);
    assert!(std::mem::size_of::<SMCKeyDataKeyInfo>() == 9);
    assert!(std::mem::size_of::<SMCParamStruct>() == 74);
};

fn key_from_str(s: &str) -> u32 {
    let b = s.as_bytes();
    ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | (b[3] as u32)
}

fn key_to_str(k: u32) -> String {
    let b = [(k >> 24) as u8, (k >> 16) as u8, (k >> 8) as u8, k as u8];
    String::from_utf8_lossy(&b).into_owned()
}


const CPU_TEMP_KEY_CANDIDATES: &[&str] = &[
    "Tp00","Tp01","Tp02","Tp03","Tp04","Tp05","Tp06","Tp07","Tp08","Tp09",
    "Tp0A","Tp0B","Tp0C","Tp0D","Tp0E","Tp0F","Tp0G","Tp0H","Tp0I","Tp0J",
    "Tp0K","Tp0L","Tp0M","Tp0N","Tp0O","Tp0P","Tp0Q","Tp0R","Tp0S","Tp0T",
    "Tp0U","Tp0V","Tp0W","Tp0X","Tp0Y","Tp0Z",
    "Tp0a","Tp0b","Tp0c","Tp0d","Tp0e","Tp0f","Tp0g","Tp0h","Tp0i","Tp0j",
    "Tp0k","Tp0l","Tp0m","Tp0n","Tp0o","Tp0p","Tp0q","Tp0r","Tp0s","Tp0t",
    "Tp10","Tp11","Tp12","Tp13","Tp14","Tp15","Tp16","Tp17","Tp18","Tp19",
    "Tp1A","Tp1B","Tp1C","Tp1D","Tp1E","Tp1F","Tp1G","Tp1H",
    "Tp1a","Tp1b","Tp1c","Tp1d","Tp1e","Tp1f","Tp1g","Tp1h",
];


pub struct Smc {
    conn: IOConnect,
}

impl Smc {
    pub fn open() -> Result<Self> {
        unsafe {
            let matching = IOServiceMatching(b"AppleSMC\0".as_ptr() as *const c_char);
            if matching.is_null() {
                return Err(anyhow!("IOServiceMatching returned null"));
            }
            let mut iter: IOObject = 0;
            let kr = IOServiceGetMatchingServices(0, matching, &mut iter);
            if kr != KERN_SUCCESS {
                return Err(anyhow!("IOServiceGetMatchingServices failed: {}", kr));
            }
            let service = IOIteratorNext(iter);
            IOObjectRelease(iter);
            if service == 0 {
                return Err(anyhow!("AppleSMC service not found (not an Apple Silicon Mac?)"));
            }
            let mut conn: IOConnect = 0;
            let kr = IOServiceOpen(service, mach_task_self(), 0, &mut conn);
            IOObjectRelease(service);
            if kr != KERN_SUCCESS {
                return Err(anyhow!("IOServiceOpen failed: {}", kr));
            }
            Ok(Smc { conn })
        }
    }

    fn call(&self, input: &SMCParamStruct, output: &mut SMCParamStruct) -> Result<()> {
        unsafe {
            let mut out_size = std::mem::size_of::<SMCParamStruct>();
            let kr = IOConnectCallStructMethod(
                self.conn,
                KERNEL_INDEX_SMC,
                input as *const _ as *const libc::c_void,
                std::mem::size_of::<SMCParamStruct>(),
                output as *mut _ as *mut libc::c_void,
                &mut out_size,
            );
            if kr != KERN_SUCCESS {
                return Err(anyhow!("IOConnectCallStructMethod failed: {}", kr));
            }
            Ok(())
        }
    }

    fn key_info(&self, key: u32) -> Result<SMCKeyDataKeyInfo> {
        let input = SMCParamStruct {
            key,
            data8: SMC_CMD_READ_KEYINFO,
            ..Default::default()
        };
        let mut output = SMCParamStruct::default();
        self.call(&input, &mut output)?;
        if output.result != 0 {
            return Err(anyhow!("key info error {:#04x}", output.result));
        }
        Ok(output.key_info)
    }

    pub fn read_flt(&self, key: u32) -> Result<f32> {
        let info = self.key_info(key)?;
        let input = SMCParamStruct {
            key,
            data8: SMC_CMD_READ_BYTES,
            key_info: info,
            ..Default::default()
        };
        let mut output = SMCParamStruct::default();
        self.call(&input, &mut output)?;
        if output.result != 0 {
            return Err(anyhow!("read error {:#04x}", output.result));
        }
        let b: [u8; SMC_DATA_SIZE] = unsafe { output.bytes.bytes };
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn write_flt(&self, key: u32, value: f32) -> Result<()> {
        let info = self.key_info(key)?;
        let mut input = SMCParamStruct {
            key,
            data8: SMC_CMD_WRITE_BYTES,
            key_info: info,
            ..Default::default()
        };
        unsafe { input.bytes.bytes[0..4].copy_from_slice(&value.to_le_bytes()) };
        let mut output = SMCParamStruct::default();
        self.call(&mut input, &mut output)?;
        if output.result != 0 {
            return Err(anyhow!("write error {:#04x}", output.result));
        }
        Ok(())
    }

    fn write_ui8(&self, key: u32, value: u8) -> Result<()> {
        let info = self.key_info(key)?;
        let mut input = SMCParamStruct {
            key,
            data8: SMC_CMD_WRITE_BYTES,
            key_info: info,
            ..Default::default()
        };
        unsafe { input.bytes.bytes[0] = value };
        let mut output = SMCParamStruct::default();
        self.call(&mut input, &mut output)?;
        if output.result != 0 {
            return Err(anyhow!("ui8 write error {:#04x}", output.result));
        }
        Ok(())
    }

    pub fn probe_cpu_temp_keys(&self) -> Vec<u32> {
        let flt_prefix = key_from_str("flt ") >> 8;
        let mut keys = Vec::new();
        for name in CPU_TEMP_KEY_CANDIDATES {
            let key = key_from_str(name);
            if let Ok(info) = self.key_info(key) {
                if (info.data_type >> 8) == flt_prefix && info.data_size == 4 {
                    if let Ok(t) = self.read_flt(key) {
                        if (10.0..130.0).contains(&t) {
                            keys.push(key);
                        }
                    }
                }
            }
        }
        keys
    }

    pub fn max_cpu_temp_over(&self, keys: &[u32]) -> Result<f32> {
        let mut max_t = 0.0f32;
        let mut found = false;
        for &key in keys {
            if let Ok(t) = self.read_flt(key) {
                if (10.0..130.0).contains(&t) {
                    if t > max_t { max_t = t; }
                    found = true;
                }
            }
        }
        if !found {
            return Err(anyhow!("CPU temp sensors stopped responding"));
        }
        Ok(max_t)
    }

    fn set_manual_mode(&self, fan: usize) -> Result<()> {
        let mode_key = key_from_str(&format!("F{}Md", fan));
        match self.write_ui8(mode_key, 1) {
            Ok(()) => Ok(()),
            Err(e) => {
                let msg = format!("{:#}", e);
                if msg.contains(&format!("{:#04x}", SMC_ERR_NOT_WRITABLE)) {
                    self.write_ui8(key_from_str("Ftst"), 1)
                        .context("Ftst unlock write failed")?;
                    thread::sleep(Duration::from_secs(3));
                    let mut last_err = anyhow!("unreachable");
                    for _ in 0..300 {
                        match self.write_ui8(mode_key, 1) {
                            Ok(()) => return Ok(()),
                            Err(e2) => { last_err = e2; thread::sleep(Duration::from_millis(100)); }
                        }
                    }
                    Err(last_err.context("manual mode still rejected after Ftst unlock"))
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Set fan 0 (and fan 1 if present) to `percent` of max RPM.
    pub fn set_fan_percent(&self, percent: f32) -> Result<()> {
        let percent = percent.clamp(0.0, 100.0);
        let fan_count = match self.read_flt(key_from_str("FNum")) {
            Ok(n) if (1.0..=4.0).contains(&n) => n as usize,
            _ => 2,
        };

        for i in 0..fan_count {
            self.set_manual_mode(i)
                .with_context(|| format!("fan {} manual mode", i))?;
            let min = self.read_flt(key_from_str(&format!("F{}Mn", i))).unwrap_or(1200.0);
            let max = self.read_flt(key_from_str(&format!("F{}Mx", i))).unwrap_or(6000.0);
            let rpm = min + (percent / 100.0) * (max - min);
            self.write_flt(key_from_str(&format!("F{}Tg", i)), rpm)
                .with_context(|| format!("fan {} target RPM", i))?;
        }
        Ok(())
    }

    /// Return all fans to automatic mode (called from main on shutdown).
    pub fn restore_auto(&self) {
        for i in 0..2 {
            let _ = self.write_ui8(key_from_str(&format!("F{}Md", i)), 0);
        }
    }
}

impl Drop for Smc {
    fn drop(&mut self) {
        unsafe { IOServiceClose(self.conn) };
    }
}

pub struct FanCurve {
    last_band: u8,
}

impl FanCurve {
    pub fn new() -> Self { Self { last_band: 0 } }

    pub fn target(&mut self, temp_c: f32) -> f32 {
        let band = if temp_c >= 80.0 { 5 }
        else if temp_c >= 75.0 { 4 }
        else if temp_c >= 70.0 { 3 }
        else if temp_c >= 60.0 { 2 }
        else if temp_c >= 50.0 { 1 }
        else if temp_c <= 48.0 { 0 }
        else { self.last_band };

        self.last_band = band;
        match band {
            5 => 100.0,
            4 => 90.0,
            3 => 80.0,
            2 => 50.0,
            1 => 30.0,
            _ => 0.0,
        }
    }
}

pub async fn spawn_fan_loop() -> Result<Smc> {
    let smc = Smc::open().context("open SMC")?;
    let poller_smc = Smc::open().context("open SMC for poller")?;
    let temp_keys = poller_smc.probe_cpu_temp_keys();
    if temp_keys.is_empty() {
        return Err(anyhow!(
            "no Tp** CPU temperature sensors found — fan control unavailable"
        ));
    }
    let names: Vec<String> = temp_keys.iter().map(|&k| key_to_str(k)).collect();
    println!(
        "🌀 fan control active ({} sensors: {} | curve 50/60/70/75/80°C -> 30/50/80/90/100%)",
        temp_keys.len(),
        names.join(" ")
    );

    let mut curve = FanCurve::new();
    tokio::task::spawn_blocking(move || {
        let mut read_error_reported = false;
        let mut write_error_reported = false;
        loop {
            match poller_smc.max_cpu_temp_over(&temp_keys) {
                Ok(t) => {
                    if read_error_reported {
                        eprintln!("🌀 temp readings recovered ({:.0}°C)", t);
                        read_error_reported = false;
                    }
                    let pct = curve.target(t);
                    if pct > 0.0 {
                        match poller_smc.set_fan_percent(pct) {
                            Ok(()) => write_error_reported = false,
                            Err(e) => {
                                if !write_error_reported {
                                    eprintln!(
                                        "🌀 fan write failed ({:.0}°C -> {:.0}%): {:#}",
                                        t, pct, e
                                    );
                                    write_error_reported = true;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    if !read_error_reported {
                        eprintln!("🌀 temp read failed: {:#}", e);
                        read_error_reported = true;
                    }
                }
            }
            thread::sleep(Duration::from_secs(2));
        }
    });
    Ok(smc)
}


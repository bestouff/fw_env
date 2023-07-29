use std::io::prelude::*;

#[derive(Debug)]
pub enum FwError {
    Io(std::io::Error),
    ParseInt(std::num::ParseIntError),
    ParseDevname,
    ParseStart,
    ParseSize,
    WrongDevNum(usize),
    BadCrc,
    EnvVarSyntax(String),
    Scan(scan_fmt::parse::ScanError)
}

impl std::fmt::Display for FwError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            FwError::Io(err) => err.fmt(f),
            _ => write!(f, "Parsing trouble"),
        }
    }
}

impl From<scan_fmt::parse::ScanError> for FwError {
    fn from(err: scan_fmt::parse::ScanError) -> FwError {
        FwError::Scan(err)
    }
}

impl From<std::io::Error> for FwError {
    fn from(err: std::io::Error) -> FwError {
        FwError::Io(err)
    }
}

impl From<std::num::ParseIntError> for FwError {
    fn from(err: std::num::ParseIntError) -> FwError {
        FwError::ParseInt(err)
    }
}

impl std::error::Error for FwError {}

#[derive(Debug, PartialEq)]
pub struct Config {
    pub line1: ConfigLine,
    pub line2: Option<ConfigLine>,
}

#[derive(Debug, PartialEq)]
pub struct ConfigLine {
    pub devname: String,
    pub start: usize,
    pub size: usize,
}

impl std::str::FromStr for ConfigLine {
    type Err = FwError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (devname, start, size) =
            scan_fmt::scan_fmt!(s, "{} 0x{x} 0x{x}", String, String, String)?;
        let start = usize::from_str_radix(&start, 16)?;
        let size = usize::from_str_radix(&size, 16)?;
        Ok(Self {
            devname,
            start,
            size,
        })
    }
}

impl Config {
    pub fn init() -> Result<Self, FwError> {
        Self::from_file("/etc/fw_env.config")
    }
    pub fn from_file<P: AsRef<std::path::Path>>(cfgpath: P) -> Result<Self, FwError> {
        let mut file = std::fs::File::open(cfgpath)?;
        let mut confile = String::new();
        file.read_to_string(&mut confile)?;
        let confile: Result<Vec<ConfigLine>, _> = confile
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .take(2)
            .map(std::str::FromStr::from_str)
            .collect();
        let mut confile = confile?;
        match confile.len() {
            1 => Ok(Self {
                line1: confile.pop().unwrap(),
                line2: None,
            }),
            2 => {
                let line2 = confile.pop();
                let line1 = confile.pop().unwrap();
                Ok(Self { line1, line2 })
            }
            _ => Err(FwError::WrongDevNum(confile.len())),
        }
    }
    pub fn is_redundant(&self) -> bool {
        self.line2.is_some()
    }
}

#[derive(Debug)]
pub struct FwEnv {
    pub vars: Vec<(Vec<u8>, Vec<u8>)>,
}

const ENV_SIMPLE_SIZE: usize = std::mem::size_of::<u32>();
const ENV_REDUNDANT_SIZE: usize = std::mem::size_of::<u32>() + std::mem::size_of::<u8>();

impl FwEnv {
    // TODO: skip bad blocks on flash
    // this will probably involve linux-specific syscalls ("nix" crate)
    fn read_block<P: AsRef<std::path::Path>>(
        path: P,
        offset: usize,
        size: usize,
    ) -> Result<Vec<u8>, FwError> {
        let mut buf = vec![0; size];
        let mut file = std::fs::File::open(path)?;
        file.seek(std::io::SeekFrom::Start(offset as u64))?;
        file.read_exact(&mut buf)?;
        Ok(buf)
    }
    // TODO: understand what the flag means, and do the whole flag dance
    // to read the appropriate block in case of redundancy
    pub fn read(config: &Config) -> Result<Self, FwError> {
        let block = Self::read_block(&config.line1.devname, config.line1.start, config.line1.size)?;
        let refcrc: u32 = unsafe { std::mem::transmute([block[0], block[1], block[2], block[3]]) };
        let skipped_bytes = if config.is_redundant() {
            ENV_REDUNDANT_SIZE
        } else {
            ENV_SIMPLE_SIZE
        };
        let crc = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
        let mut digest = crc.digest();
        digest.update(&block[skipped_bytes..]);
        let result = digest.finalize();
        if result != refcrc {
            return Err(FwError::BadCrc);
        }
        let mut vars = Vec::new();
        for s in block[skipped_bytes..]
            .split(|&c| c == 0)
            .take_while(|s| !s.is_empty())
        {
            let pos = s
                .iter()
                .position(|&c| c == b'=')
                .ok_or_else(|| FwError::EnvVarSyntax(String::from_utf8_lossy(s).to_string()))?;
            vars.push((s[..pos].to_vec(), s[pos + 1..].to_vec()));
        }
        Ok(Self { vars })
    }
    pub fn find_var<'a, 'b>(&'a self, name: impl Into<&'b [u8]>) -> Option<&'a [u8]> {
        let name = name.into();
        self.vars
            .iter()
            .find(|(v, _)| v[..] == name[..])
            .map(|(_, t)| &t[..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_file() {
        let config = Config::from_file("testfiles/fw_env.config");
        assert_eq!(
            config.unwrap(),
            Config {
                line1: ConfigLine {
                    devname: "/dev/mmcblk1".to_string(),
                    start: 0x180000,
                    size: 0x20000
                },
                line2: Some(ConfigLine {
                    devname: "/dev/mmcblk1".to_string(),
                    start: 0x1A0000,
                    size: 0x20000
                })
            }
        );
    }

    #[test]
    fn test_fwenv_read_block() {
        let refblock = include_bytes!("../testfiles/fw_env_gt187908");
        let envblock = FwEnv::read_block("testfiles/fw_env_gt187908", 0, 0x20000).unwrap();
        assert_eq!(envblock[..], refblock[..]);
    }

    #[test]
    fn test_fwenv_find_var() {
        let mut config = Config::from_file("testfiles/fw_env.config").unwrap();
        config.line1.devname = "testfiles/fw_env_gt187908".to_string();
        config.line1.start = 0;
        let env = FwEnv::read(&config).unwrap();
        assert_eq!(env.find_var(&b"version_os_b"[..]), Some(&b"20181217"[..]));
    }
}

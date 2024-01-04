use byteorder::{BigEndian, ReadBytesExt};
use bytes::{Bytes, BytesMut};

#[derive(Clone, Debug)]
pub struct MpegTsPacket {
    pub data: Bytes
}

impl MpegTsPacket {
    pub fn new() -> Self {
        Self {
            data: Bytes::new()
        }
    }

    pub fn from_bytes(bytes: Bytes) -> Self {
        Self {
            data: bytes
        }
    }

    pub fn payload_unit_start_indicator(&self) -> bool {
        self.data[1] & 0x40 != 0
    }

    pub fn priority(&self) -> bool {
        self.data[1] & 0x20 != 0
    }

    pub fn pid(&self) -> u16 {
        (((self.data[1] as u16) << 8) | self.data[2] as u16) & 0x1FFF
    }

    pub fn transport_scrambling_control(&self) -> u8 {
        (self.data[3] >> 6) & 0b11
    }

    pub fn adaptation_field_control(&self) -> u8 {
        (self.data[3] >> 4) & 0b11
    }

    pub fn psi(&self) -> Result<Psi, std::io::Error> {
        let data = if self.payload_unit_start_indicator() {
            self.data.slice((5 + self.data[4] as usize)..)
        } else {
            self.data.slice(5..)
        };
        Psi::decode(&data)
    }

    pub fn is_pat(&self) -> bool {
        self.pid() == 0x0000
    }
}

pub struct Psi {
    pub id: u8,
    pub syntax: Option<PsiSyntax>,
    pub pats: Option<Vec<ProgramAssociationTable>>,
    pub pmts: Option<Vec<ProgramMapTable>>
}

impl Psi {
    pub fn decode(mut buf: &[u8]) -> Result<Self, std::io::Error> {
        let id = buf.read_u8()?;
        let rest_header = buf.read_u16::<BigEndian>()?;
        let has_syntax = (rest_header >> 15) & 0b1 == 1;
        let section_len = rest_header & 0x3FF;
        let syntax = if has_syntax {
            Some(PsiSyntax::decode(&mut buf)?)
        } else {
            None
        };
        let data_len = if has_syntax { section_len - 9 } else { section_len };
        let pats = if data_len > 0 && id == 0x0 {
            let mut pats = Vec::with_capacity(1);
            pats.push(ProgramAssociationTable::decode(&mut buf)?);
            Some(pats)
        } else {
            None
        };
        let pmts = if data_len > 0 && id == 0x2 {
            let mut pmts = Vec::with_capacity(1);
            pmts.push(ProgramMapTable::decode(&mut buf)?);
            Some(pmts)
        } else {
            None
        };
        Ok(Self {
            id,
            syntax,
            pats,
            pmts
        })
    }
}

#[derive(Debug)]
pub struct PsiSyntax {
    pub transport_stream_id: u16
}

impl PsiSyntax {
    pub fn decode(buf: &mut &[u8]) -> Result<Self, std::io::Error> {
        let transport_stream_id = buf.read_u16::<BigEndian>()?;
        let _ = buf.read_u8()?;
        let _ = buf.read_u8()?;
        let _ = buf.read_u8()?;

        Ok(Self {
            transport_stream_id
        })
    }
}

#[derive(Debug)]
pub struct ProgramAssociationTable {
    pub program_num: u16,
    pub pmt_pid: u16
}

impl ProgramAssociationTable {
    pub fn decode(buf: &mut &[u8]) -> Result<Self, std::io::Error> {
        let program_num = buf.read_u16::<BigEndian>()?;
        let pmt_pid = buf.read_u16::<BigEndian>()? & 0x1FFF;
        Ok(Self {
            program_num,
            pmt_pid
        })
    }
}

#[derive(Debug)]
pub struct ProgramMapTable {
    pub pcr_pid: Option<u16>,
    //pub program_defs: Vec<ProgramDefinition>
}

impl ProgramMapTable {
    pub fn decode(buf: &mut &[u8]) -> Result<Self, std::io::Error> {
        let pcr_pid = match buf.read_u16::<BigEndian>()? & 0x1FFF {
            0x1FFF => None,
            pid    => Some(pid)
        };
        let program_info_len = (buf.read_u16::<BigEndian>()? & 0x3FF) as usize;
        println!("program_info_len = {}", program_info_len);
        //buf = &buf[program_info_len..];

        Ok(Self {
            pcr_pid
        })
    }
}

#[derive(Clone)]
pub struct SrtPayload {
    pub packets: [MpegTsPacket; 7]
}

impl SrtPayload {
    pub fn new() -> Self {
        Self {
            packets: [
                MpegTsPacket::new(),
                MpegTsPacket::new(),
                MpegTsPacket::new(),
                MpegTsPacket::new(),
                MpegTsPacket::new(),
                MpegTsPacket::new(),
                MpegTsPacket::new()
            ]
        }
    }

    pub fn decode(mut buf: Bytes) -> Result<Self, std::io::Error> {
        let mut payload = Self::new();
        for packet in payload.packets.iter_mut() {
            *packet = MpegTsPacket::from_bytes(buf.split_to(188));
        }
        Ok(payload)
    }

    pub fn encode(&self, buf: &mut BytesMut) -> Result<(), std::io::Error> {
        for packet in &self.packets {
            buf.extend_from_slice(&packet.data);
        }
        Ok(())
    }
}

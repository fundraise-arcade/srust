use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::{Bytes, BytesMut};
use crate::error::*;
use crc_all::Crc;
use std::io::Write;

#[derive(Clone, Debug)]
pub struct SrtPayload {
    pub packets: Vec<MpegTsPacket>
}

impl SrtPayload {
    pub fn decode(buf: &mut &[u8]) -> Result<Self> {
        let mut packets = Vec::with_capacity(7);
        for i in 0..7 {
            let buf = &buf[188*i..188*(i+1)];
            match MpegTsPacket::decode(&buf) {
                Ok(packet) => { packets.push(packet); },
                Err(Error::NoSync) => { break; },
                Err(err) => { return Err(err); }
            }
        }
        Ok(SrtPayload {
            packets
        })
    }

    pub fn encode(&self, buf: &mut &mut [u8]) -> Result<()> {
        let mut i = 0;
        for packet in &self.packets {
            let mut tmp_buf: &mut [u8] = &mut buf[188*i..188*(i+1)];
            packet.encode(&mut tmp_buf)?;
            i += 1;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct MpegTsPacket {
    pub transport_error_indicator: bool,
    pub payload_unit_start_indicator: bool,
    pub transport_priority: bool,
    pub pid: u16,
    pub transport_scrambling_control: u8,
    pub adaptation_field_control: u8,
    pub continuity_counter: u8,
    pub adaptation_field: Option<AdaptationField>,
    pub payload: Option<Bytes>
}

impl MpegTsPacket {
    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        let sync_byte = buf.read_u8()?;
        if sync_byte != 0x47 {
            return Err(Error::NoSync);
        }

        let flags = buf.read_u16::<BigEndian>()?;
        let transport_error_indicator = ((flags >> 15) & 0b1) != 0;
        let payload_unit_start_indicator = ((flags >> 14) & 0b1) != 0;
        let transport_priority = ((flags >> 13) & 0b1) != 0;
        let pid = flags & 0x1FFF;
        let control = buf.read_u8()?;
        let transport_scrambling_control = (control >> 6) & 0b11;
        let adaptation_field_control = (control >> 4) & 0b11;
        let continuity_counter = control & 0xF;

        let adaptation_field = if adaptation_field_control == 0b10 || adaptation_field_control == 0b11 {
            Some(AdaptationField::decode(&mut buf)?)
        } else {
            None
        };

        let payload = if adaptation_field_control == 0b01 || adaptation_field_control == 0b11 {
            Some(Bytes::copy_from_slice(buf))
        } else {
            None
        };

        Ok(Self {
            transport_error_indicator,
            payload_unit_start_indicator,
            transport_priority,
            pid,
            transport_scrambling_control,
            adaptation_field_control,
            continuity_counter,
            adaptation_field,
            payload
        })
    }

    pub fn encode(&self, buf: &mut &mut [u8]) -> Result<()> {
        buf.write_u8(0x47)?;
        let flags = ((self.transport_error_indicator as u16) << 15) |
                    ((self.payload_unit_start_indicator as u16) << 14) |
                    self.pid;
        buf.write_u16::<BigEndian>(flags)?;
        let control = (self.transport_scrambling_control << 6) |
                      (self.adaptation_field_control << 4) |
                      self.continuity_counter;
        buf.write_u8(control)?;
        if let Some(adaptation_field) = &self.adaptation_field {
            adaptation_field.encode(buf)?;
        }
        if let Some(payload) = &self.payload {
            buf.write_all(&payload)?;
        }
        Ok(())
    }

    pub fn psi(&self) -> Result<Psi> {
        match &self.payload {
            Some(payload) => {
                let mut buf: &[u8] = &payload;
                Psi::decode(self, &mut buf)
            },
            None => Err(Error::NoPayload)
        }
    }

    pub fn set_psi(&mut self, psi: Psi) -> Result<()> {
        let mut buf = BytesMut::zeroed(psi.encoded_len());
        let mut slice: &mut [u8] = &mut buf;
        psi.encode(&mut slice)?;
        self.payload = Some(buf.freeze());

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct AdaptationField {
    data: Bytes
}

impl AdaptationField {
    pub fn decode(buf: &mut &[u8]) -> Result<Self> {
        let len = buf.read_u8()? as usize;
        let data = Bytes::copy_from_slice(&buf[..len]);
        *buf = &buf[len..];

        Ok(Self {
            data
        })
    }

    pub fn encode(&self, buf: &mut &mut [u8]) -> Result<()> {
        buf.write_u8(self.data.len() as u8)?;
        buf.write_all(&self.data)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Psi {
    pub pointer_field: Option<usize>,
    pub id: u8,
    pub syntax: Option<PsiSyntax>,
    pub data: Option<PsiData>,
}

impl Psi {
    pub fn decode(packet: &MpegTsPacket, buf: &mut &[u8]) -> Result<Self> {
        let pointer_field = if packet.payload_unit_start_indicator {
            Some(buf.read_u8()? as usize)
        } else {
            None
        };

        let id = buf.read_u8()?;
        let rest_header = buf.read_u16::<BigEndian>()?;
        let has_syntax = (rest_header >> 15) & 0b1 != 0;
        let section_len = (rest_header & 0x3FF) as usize;
        let syntax = if has_syntax {
            Some(PsiSyntax::decode(buf)?)
        } else {
            None
        };

        // drop syntax header and final CRC from len
        let data_len = if has_syntax { section_len - 9 } else { section_len };
        let mut data_buf = &buf[..data_len];

        let data = if data_len > 0 {
            Some(match id {
                0x0 => PsiData::Pat(ProgramAssociationTable::decode(&mut data_buf)?),
                0x2 => PsiData::Pmt(ProgramMapTable::decode(&mut data_buf)?),
                _ => panic!()
            })
        } else {
            None
        };

        Ok(Self {
            pointer_field,
            id,
            syntax,
            data,
        })
    }

    pub fn encode(&self, buf: &mut &mut [u8]) -> Result<()> {
        if let Some(pointer_field) = self.pointer_field {
            buf.write_u8(pointer_field as u8)?;
            for _ in 0..pointer_field {
                buf.write_u8(0xFF)?;
            }
        }

        // subtract CRC32 from len
        let tmp_len = buf.len() - if self.syntax.is_some() { 4 } else { 0 };
        let mut tmp_buf = BytesMut::zeroed(tmp_len);
        let mut slice: &mut [u8] = &mut tmp_buf;

        slice.write_u8(self.id)?;

        let remainder_len = self.section_len() + if self.syntax.is_some() { 4 } else { 0 };
        let rest_header = ((self.syntax.is_some() as u16) << 15) |
                          (0b0 << 14) |
                          (0b11 << 12) |
                          (0b00 << 10) |
                          (remainder_len as u16);
        slice.write_u16::<BigEndian>(rest_header)?;

        if let Some(syntax) = &self.syntax {
            syntax.encode(&mut slice)?;
        }

        if let Some(data) = &self.data {
            match data {
                PsiData::Pat(pat) => pat.encode(&mut slice)?,
                PsiData::Pmt(pmt) => pmt.encode(&mut slice)?
            }
        }

        // id + rest of header + section
        let crc32_len = 1 + 2 + self.section_len();
        let crc32_buf = &tmp_buf[..crc32_len];
        buf.write_all(crc32_buf)?;

        if self.syntax.is_some() {
            let mut crc32_mpeg = Crc::<u32>::new(0x04C11DB7, 32, 0xFFFFFFFF, 0x00000000, false);
            crc32_mpeg.update(crc32_buf);
            buf.write_u32::<BigEndian>(crc32_mpeg.finish())?;
        }

        Ok(())
    }

    pub fn encoded_len(&self) -> usize {
        let mut result = 0;
        if let Some(pointer_field) = self.pointer_field {
            result += 1 + pointer_field;
        }

        // id + rest of header + section
        result += 1 + 2 + self.section_len();

        // CRC32
        if self.syntax.is_some() {
            result += 4;
        }

        result
    }

    pub fn section_len(&self) -> usize {
        let mut result = 0;

        if let Some(syntax) = &self.syntax {
            result += syntax.encoded_len();
        }

        if let Some(data) = &self.data {
            result += match data {
                PsiData::Pat(pat) => pat.encoded_len(),
                PsiData::Pmt(pmt) => pmt.encoded_len()
            };
        }

        result
    }
}

#[derive(Clone, Debug)]
pub enum PsiData {
    Pat(ProgramAssociationTable),
    Pmt(ProgramMapTable)
}

#[derive(Clone, Debug)]
pub struct PsiSyntax {
    pub transport_stream_id: u16,
    pub version: u8,
    pub is_current: bool,
    pub section: u8,
    pub final_section: u8
}

impl PsiSyntax {
    pub fn decode(buf: &mut &[u8]) -> Result<Self> {
        let transport_stream_id = buf.read_u16::<BigEndian>()?;
        let flags = buf.read_u8()?;
        let version = (flags >> 1) & 0x1F;
        let is_current = flags & 0b1 == 1;
        let section = buf.read_u8()?;
        let final_section = buf.read_u8()?;

        Ok(Self {
            transport_stream_id,
            version,
            is_current,
            section,
            final_section
        })
    }

    pub fn encode(&self, buf: &mut &mut [u8]) -> Result<()> {
        buf.write_u16::<BigEndian>(self.transport_stream_id)?;

        let flags = (0b11 << 6) |
                    (self.version << 1) |
                    (self.is_current as u8);
        buf.write_u8(flags)?;
        buf.write_u8(self.section)?;
        buf.write_u8(self.final_section)?;

        Ok(())
    }

    pub fn encoded_len(&self) -> usize {
        let mut result = 0;

        // transport stream ID + flags + section num + final section num
        result += 2 + 1 + 1 + 1;

        result
    }
}

#[derive(Clone, Debug)]
pub struct ProgramAssociationTable {
    pub program_num: u16,
    pub pmt_pid: u16
}

impl ProgramAssociationTable {
    pub fn decode(buf: &mut &[u8]) -> Result<Self> {
        let program_num = buf.read_u16::<BigEndian>()?;
        let pmt_pid = buf.read_u16::<BigEndian>()? & 0x1FFF;
        Ok(Self {
            program_num,
            pmt_pid
        })
    }

    pub fn encode(&self, buf: &mut &mut [u8]) -> Result<()> {
        buf.write_u16::<BigEndian>(self.program_num)?;
        buf.write_u16::<BigEndian>((0b111 << 13) | self.pmt_pid)?;
        Ok(())
    }

    pub fn encoded_len(&self) -> usize {
        2 + 2
    }
}

#[derive(Clone, Debug)]
pub struct ProgramMapTable {
    pub pcr_pid: Option<u16>,
    pub program_descriptors: Bytes,
    pub program_definitions: Vec<ProgramDefinition>
}

impl ProgramMapTable {
    pub fn decode(buf: &mut &[u8]) -> Result<Self> {
        let pcr_pid = match buf.read_u16::<BigEndian>()? & 0x1FFF {
            0x1FFF => None,
            pid    => Some(pid)
        };
        let program_descriptors_len = (buf.read_u16::<BigEndian>()? & 0x3FF) as usize;

        let program_descriptors = if program_descriptors_len > 0 {
            Bytes::copy_from_slice(&buf[..program_descriptors_len])
        } else {
            Bytes::new()
        };

        *buf = &buf[program_descriptors_len..];

        let mut program_definitions = Vec::with_capacity(3);
        while buf.len() > 0 {
            program_definitions.push(ProgramDefinition::decode(buf)?);
        }

        Ok(Self {
            pcr_pid,
            program_descriptors,
            program_definitions
        })
    }

    pub fn encode(&self, buf: &mut &mut [u8]) -> Result<()> {
        let pcr_pid = match self.pcr_pid {
            Some(pid) => pid,
            None => 0x1FFF
        };
        buf.write_u16::<BigEndian>((0b111 << 13) | pcr_pid)?;
        buf.write_u16::<BigEndian>((0b1111 << 12) |
                                   (0b00 << 10) |
                                   (self.program_descriptors.len() as u16))?;
        buf.write_all(&self.program_descriptors)?;

        for program_definition in &self.program_definitions {
            program_definition.encode(buf)?;
        }

        Ok(())
    }

    pub fn encoded_len(&self) -> usize {
        let mut result = 0;

        // PCR PID + program descriptors len + program descriptors
        result += 2 + 2 + self.program_descriptors.len();

        for program_definition in &self.program_definitions {
            result += program_definition.encoded_len();
        }

        result
    }
}

#[derive(Clone, Debug)]
pub struct ProgramDefinition {
    pub stream_type: u8,
    pub stream_pid: u16,
    pub stream_descriptors: Bytes
}

impl ProgramDefinition {
    pub fn decode(buf: &mut &[u8]) -> Result<Self> {
        let stream_type = buf.read_u8()?;
        let stream_pid = buf.read_u16::<BigEndian>()? & 0x1FFF;
        let stream_descriptors_len = (buf.read_u16::<BigEndian>()? & 0x3FF) as usize;

        let stream_descriptors = if stream_descriptors_len > 0 {
            Bytes::copy_from_slice(&buf[..stream_descriptors_len])
        } else {
            Bytes::new()
        };

        *buf = &buf[stream_descriptors_len..];

        Ok(Self {
            stream_type,
            stream_pid,
            stream_descriptors
        })
    }

    pub fn encode(&self, buf: &mut &mut [u8]) -> Result<()> {
        buf.write_u8(self.stream_type)?;
        buf.write_u16::<BigEndian>((0b111 << 13) | self.stream_pid)?;
        buf.write_u16::<BigEndian>((0b1111 << 12) |
                                   (0b00 << 10) |
                                   (self.stream_descriptors.len() as u16))?;
        buf.write_all(&self.stream_descriptors)?;
        Ok(())
    }

    pub fn encoded_len(&self) -> usize {
        let mut result = 0;

        // type + elementary PID + desc len + desc
        result += 1 + 2 + 2 + self.stream_descriptors.len();

        result
    }
}

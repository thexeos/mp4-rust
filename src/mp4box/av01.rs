use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use serde::Serialize;
use std::io::{Read, Seek, Write};

use crate::mp4box::*;
use crate::types::Av1Config;

/// AV1 Sample Entry box (`av01`).
///
/// Defined in the AV1 Codec ISO Media File Format Binding:
/// <https://aomediacodec.github.io/av1-isobmff/>
///
/// The sample entry uses the standard VisualSampleEntry base layout
/// (same 78-byte prefix as `avc1` / `hev1`) with an `av1C` child box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Av01Box {
    pub data_reference_index: u16,
    pub width: u16,
    pub height: u16,

    #[serde(with = "value_u32")]
    pub horizresolution: FixedPointU16,

    #[serde(with = "value_u32")]
    pub vertresolution: FixedPointU16,
    pub frame_count: u16,
    pub depth: u16,
    pub av1c: Av1CBox,
}

impl Default for Av01Box {
    fn default() -> Self {
        Av01Box {
            data_reference_index: 0,
            width: 0,
            height: 0,
            horizresolution: FixedPointU16::new(0x48),
            vertresolution: FixedPointU16::new(0x48),
            frame_count: 1,
            depth: 0x0018,
            av1c: Av1CBox::default(),
        }
    }
}

impl Av01Box {
    pub fn new(config: &Av1Config) -> Self {
        Av01Box {
            data_reference_index: 1,
            width: config.width,
            height: config.height,
            horizresolution: FixedPointU16::new(0x48),
            vertresolution: FixedPointU16::new(0x48),
            frame_count: 1,
            depth: 0x0018,
            av1c: Av1CBox::default(),
        }
    }

    pub fn get_type(&self) -> BoxType {
        BoxType::Av01Box
    }

    pub fn get_size(&self) -> u64 {
        HEADER_SIZE + 8 + 70 + self.av1c.box_size()
    }
}

impl Mp4Box for Av01Box {
    fn box_type(&self) -> BoxType {
        self.get_type()
    }

    fn box_size(&self) -> u64 {
        self.get_size()
    }

    fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(&self).unwrap())
    }

    fn summary(&self) -> Result<String> {
        let s = format!(
            "data_reference_index={} width={} height={} frame_count={}",
            self.data_reference_index, self.width, self.height, self.frame_count
        );
        Ok(s)
    }
}

impl<R: Read + Seek> ReadBox<&mut R> for Av01Box {
    fn read_box(reader: &mut R, size: u64) -> Result<Self> {
        let start = box_start(reader)?;

        reader.read_u32::<BigEndian>()?; // reserved
        reader.read_u16::<BigEndian>()?; // reserved
        let data_reference_index = reader.read_u16::<BigEndian>()?;

        reader.read_u32::<BigEndian>()?; // pre-defined, reserved
        reader.read_u64::<BigEndian>()?; // pre-defined
        reader.read_u32::<BigEndian>()?; // pre-defined
        let width = reader.read_u16::<BigEndian>()?;
        let height = reader.read_u16::<BigEndian>()?;
        let horizresolution = FixedPointU16::new_raw(reader.read_u32::<BigEndian>()?);
        let vertresolution = FixedPointU16::new_raw(reader.read_u32::<BigEndian>()?);
        reader.read_u32::<BigEndian>()?; // reserved
        let frame_count = reader.read_u16::<BigEndian>()?;
        skip_bytes(reader, 32)?; // compressorname
        let depth = reader.read_u16::<BigEndian>()?;
        reader.read_i16::<BigEndian>()?; // pre-defined

        let header = BoxHeader::read(reader)?;
        let BoxHeader { name, size: s } = header;
        if s > size {
            return Err(Error::InvalidData(
                "av01 box contains a box with a larger size than it",
            ));
        }
        if name == BoxType::Av1CBox {
            let av1c = Av1CBox::read_box(reader, s)?;

            skip_bytes_to(reader, start + size)?;

            Ok(Av01Box {
                data_reference_index,
                width,
                height,
                horizresolution,
                vertresolution,
                frame_count,
                depth,
                av1c,
            })
        } else {
            Err(Error::InvalidData("av1C not found"))
        }
    }
}

impl<W: Write> WriteBox<&mut W> for Av01Box {
    fn write_box(&self, writer: &mut W) -> Result<u64> {
        let size = self.box_size();
        BoxHeader::new(self.box_type(), size).write(writer)?;

        writer.write_u32::<BigEndian>(0)?; // reserved
        writer.write_u16::<BigEndian>(0)?; // reserved
        writer.write_u16::<BigEndian>(self.data_reference_index)?;

        writer.write_u32::<BigEndian>(0)?; // pre-defined, reserved
        writer.write_u64::<BigEndian>(0)?; // pre-defined
        writer.write_u32::<BigEndian>(0)?; // pre-defined
        writer.write_u16::<BigEndian>(self.width)?;
        writer.write_u16::<BigEndian>(self.height)?;
        writer.write_u32::<BigEndian>(self.horizresolution.raw_value())?;
        writer.write_u32::<BigEndian>(self.vertresolution.raw_value())?;
        writer.write_u32::<BigEndian>(0)?; // reserved
        writer.write_u16::<BigEndian>(self.frame_count)?;
        // skip compressorname
        write_zeros(writer, 32)?;
        writer.write_u16::<BigEndian>(self.depth)?;
        writer.write_i16::<BigEndian>(-1)?; // pre-defined

        self.av1c.write_box(writer)?;

        Ok(size)
    }
}

/// AV1 Codec Configuration Box (`av1C`).
///
/// Contains the AV1CodecConfigurationRecord: 4 bytes of fixed bitfields
/// followed by zero or more configOBUs (typically a Sequence Header OBU).
///
/// Layout (from the AV1-ISOBMFF spec §2.3.3):
/// ```text
/// unsigned int (1) marker = 1;
/// unsigned int (7) version = 1;
/// unsigned int (3) seq_profile;
/// unsigned int (5) seq_level_idx_0;
/// unsigned int (1) seq_tier_0;
/// unsigned int (1) high_bitdepth;
/// unsigned int (1) twelve_bit;
/// unsigned int (1) monochrome;
/// unsigned int (1) chroma_subsampling_x;
/// unsigned int (1) chroma_subsampling_y;
/// unsigned int (2) chroma_sample_position;
/// unsigned int (3) reserved = 0;
/// unsigned int (1) initial_presentation_delay_present;
/// unsigned int (4) initial_presentation_delay_minus_one / reserved;
/// unsigned int (8)[] configOBUs;
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Av1CBox {
    pub seq_profile: u8,
    pub seq_level_idx_0: u8,
    pub seq_tier_0: bool,
    pub high_bitdepth: bool,
    pub twelve_bit: bool,
    pub monochrome: bool,
    pub chroma_subsampling_x: bool,
    pub chroma_subsampling_y: bool,
    pub chroma_sample_position: u8,
    pub initial_presentation_delay_present: bool,
    pub initial_presentation_delay_minus_one: u8,
    /// Raw OBU bytes (typically a Sequence Header OBU).
    #[serde(skip)]
    pub config_obus: Vec<u8>,
}

impl Mp4Box for Av1CBox {
    fn box_type(&self) -> BoxType {
        BoxType::Av1CBox
    }

    fn box_size(&self) -> u64 {
        HEADER_SIZE + 4 + self.config_obus.len() as u64
    }

    fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(&self).unwrap())
    }

    fn summary(&self) -> Result<String> {
        Ok(format!(
            "profile={} level={} tier={}",
            self.seq_profile, self.seq_level_idx_0, self.seq_tier_0 as u8
        ))
    }
}

impl<R: Read + Seek> ReadBox<&mut R> for Av1CBox {
    fn read_box(reader: &mut R, size: u64) -> Result<Self> {
        let start = box_start(reader)?;

        let byte0 = reader.read_u8()?;
        let _marker = (byte0 >> 7) & 1; // must be 1
        let _version = byte0 & 0x7F; // must be 1

        let byte1 = reader.read_u8()?;
        let seq_profile = (byte1 >> 5) & 0x07;
        let seq_level_idx_0 = byte1 & 0x1F;

        let byte2 = reader.read_u8()?;
        let seq_tier_0 = (byte2 >> 7) & 1 == 1;
        let high_bitdepth = (byte2 >> 6) & 1 == 1;
        let twelve_bit = (byte2 >> 5) & 1 == 1;
        let monochrome = (byte2 >> 4) & 1 == 1;
        let chroma_subsampling_x = (byte2 >> 3) & 1 == 1;
        let chroma_subsampling_y = (byte2 >> 2) & 1 == 1;
        let chroma_sample_position = byte2 & 0x03;

        let byte3 = reader.read_u8()?;
        let initial_presentation_delay_present = (byte3 >> 4) & 1 == 1;
        let initial_presentation_delay_minus_one = if initial_presentation_delay_present {
            byte3 & 0x0F
        } else {
            0
        };

        // Remaining bytes are configOBUs
        let config_obus_size = size
            .checked_sub(HEADER_SIZE + 4)
            .ok_or(Error::InvalidData("av1C size too small"))?;
        let mut config_obus = vec![0u8; config_obus_size as usize];
        reader.read_exact(&mut config_obus)?;

        skip_bytes_to(reader, start + size)?;

        Ok(Av1CBox {
            seq_profile,
            seq_level_idx_0,
            seq_tier_0,
            high_bitdepth,
            twelve_bit,
            monochrome,
            chroma_subsampling_x,
            chroma_subsampling_y,
            chroma_sample_position,
            initial_presentation_delay_present,
            initial_presentation_delay_minus_one,
            config_obus,
        })
    }
}

impl<W: Write> WriteBox<&mut W> for Av1CBox {
    fn write_box(&self, writer: &mut W) -> Result<u64> {
        let size = self.box_size();
        BoxHeader::new(self.box_type(), size).write(writer)?;

        // byte 0: marker(1) = 1 | version(7) = 1
        writer.write_u8(0x81)?;

        // byte 1: seq_profile(3) | seq_level_idx_0(5)
        writer.write_u8((self.seq_profile << 5) | (self.seq_level_idx_0 & 0x1F))?;

        // byte 2: seq_tier_0(1) | high_bitdepth(1) | twelve_bit(1) | monochrome(1)
        //         | chroma_subsampling_x(1) | chroma_subsampling_y(1) | chroma_sample_position(2)
        let byte2 = ((self.seq_tier_0 as u8) << 7)
            | ((self.high_bitdepth as u8) << 6)
            | ((self.twelve_bit as u8) << 5)
            | ((self.monochrome as u8) << 4)
            | ((self.chroma_subsampling_x as u8) << 3)
            | ((self.chroma_subsampling_y as u8) << 2)
            | (self.chroma_sample_position & 0x03);
        writer.write_u8(byte2)?;

        // byte 3: reserved(3) = 0 | initial_presentation_delay_present(1)
        //         | initial_presentation_delay_minus_one(4) / reserved(4) = 0
        let byte3 = if self.initial_presentation_delay_present {
            (1 << 4) | (self.initial_presentation_delay_minus_one & 0x0F)
        } else {
            0
        };
        writer.write_u8(byte3)?;

        // configOBUs
        writer.write_all(&self.config_obus)?;

        Ok(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mp4box::BoxHeader;
    use std::io::Cursor;

    #[test]
    fn test_av1c_round_trip() {
        let src_box = Av1CBox {
            seq_profile: 0,
            seq_level_idx_0: 4,
            seq_tier_0: false,
            high_bitdepth: false,
            twelve_bit: false,
            monochrome: false,
            chroma_subsampling_x: true,
            chroma_subsampling_y: true,
            chroma_sample_position: 0,
            initial_presentation_delay_present: false,
            initial_presentation_delay_minus_one: 0,
            config_obus: vec![
                0x0a, 0x0b, 0x00, 0x00, 0x00, 0x24, 0xcf, 0x7f, 0x0d, 0xbf, 0xff, 0x30, 0x08,
            ],
        };

        let mut buf = Vec::new();
        src_box.write_box(&mut buf).unwrap();
        assert_eq!(buf.len(), src_box.box_size() as usize);

        let mut reader = Cursor::new(&buf);
        let header = BoxHeader::read(&mut reader).unwrap();
        assert_eq!(header.name, BoxType::Av1CBox);
        assert_eq!(src_box.box_size(), header.size);

        let dst_box = Av1CBox::read_box(&mut reader, header.size).unwrap();
        assert_eq!(src_box, dst_box);
    }

    #[test]
    fn test_av01_round_trip() {
        let src_box = Av01Box {
            data_reference_index: 1,
            width: 960,
            height: 540,
            horizresolution: FixedPointU16::new(0x48),
            vertresolution: FixedPointU16::new(0x48),
            frame_count: 1,
            depth: 24,
            av1c: Av1CBox {
                seq_profile: 0,
                seq_level_idx_0: 4,
                seq_tier_0: false,
                high_bitdepth: false,
                twelve_bit: false,
                monochrome: false,
                chroma_subsampling_x: true,
                chroma_subsampling_y: true,
                chroma_sample_position: 0,
                initial_presentation_delay_present: false,
                initial_presentation_delay_minus_one: 0,
                config_obus: vec![0x0a, 0x0b],
            },
        };

        let mut buf = Vec::new();
        src_box.write_box(&mut buf).unwrap();
        assert_eq!(buf.len(), src_box.box_size() as usize);

        let mut reader = Cursor::new(&buf);
        let header = BoxHeader::read(&mut reader).unwrap();
        assert_eq!(header.name, BoxType::Av01Box);
        assert_eq!(src_box.box_size(), header.size);

        let dst_box = Av01Box::read_box(&mut reader, header.size).unwrap();
        assert_eq!(src_box, dst_box);
    }

    #[test]
    fn test_av1c_with_presentation_delay() {
        let src_box = Av1CBox {
            seq_profile: 1,
            seq_level_idx_0: 8,
            seq_tier_0: true,
            high_bitdepth: true,
            twelve_bit: false,
            monochrome: false,
            chroma_subsampling_x: true,
            chroma_subsampling_y: false,
            chroma_sample_position: 1,
            initial_presentation_delay_present: true,
            initial_presentation_delay_minus_one: 3,
            config_obus: vec![],
        };

        let mut buf = Vec::new();
        src_box.write_box(&mut buf).unwrap();

        let mut reader = Cursor::new(&buf);
        let header = BoxHeader::read(&mut reader).unwrap();
        let dst_box = Av1CBox::read_box(&mut reader, header.size).unwrap();
        assert_eq!(src_box, dst_box);
    }
}

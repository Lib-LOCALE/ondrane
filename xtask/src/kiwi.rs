//! Minimal decoder for the Kiwi binary format used by Figma `.fig` files
//! (`fig-kiwi` header, deflate-compressed schema, zstd/deflate-compressed
//! message). Produces a dynamic [`Value`] tree.

use std::collections::BTreeMap;
use std::io::Read;

#[derive(Debug, Clone)]
pub enum Value {
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f32),
    Str(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
    Enum(String),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(m) => m.get(key),
            _ => None,
        }
    }
    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    pub fn f(&self, key: &str) -> Option<f64> {
        match self.get(key)? {
            Value::Float(v) => Some(*v as f64),
            Value::Int(v) => Some(*v as f64),
            Value::UInt(v) => Some(*v as f64),
            _ => None,
        }
    }
    pub fn u(&self, key: &str) -> Option<u64> {
        match self.get(key)? {
            Value::UInt(v) => Some(*v),
            Value::Int(v) => Some(*v as u64),
            _ => None,
        }
    }
    pub fn b(&self, key: &str) -> Option<bool> {
        match self.get(key)? {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }
    pub fn s(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Str(v) | Value::Enum(v) => Some(v),
            _ => None,
        }
    }
    pub fn arr(&self, key: &str) -> &[Value] {
        match self.get(key) {
            Some(Value::Array(v)) => v,
            _ => &[],
        }
    }
    pub fn bytes(&self, key: &str) -> Option<&[u8]> {
        match self.get(key)? {
            Value::Bytes(v) => Some(v),
            _ => None,
        }
    }
}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn byte(&mut self) -> Result<u8, String> {
        let b = *self.d.get(self.p).ok_or("unexpected end of kiwi data")?;
        self.p += 1;
        Ok(b)
    }
    fn var_uint(&mut self) -> Result<u64, String> {
        let (mut r, mut shift) = (0u64, 0u32);
        loop {
            let b = self.byte()?;
            r |= ((b & 127) as u64) << shift;
            shift += 7;
            if b & 128 == 0 || shift >= 64 {
                return Ok(r);
            }
        }
    }
    fn var_int(&mut self) -> Result<i64, String> {
        let v = self.var_uint()?;
        Ok(((v >> 1) as i64) ^ -((v & 1) as i64))
    }
    fn float(&mut self) -> Result<f32, String> {
        let b0 = self.byte()?;
        if b0 == 0 {
            return Ok(0.0);
        }
        let mut bits = b0 as u32;
        for i in 1..4 {
            bits |= (self.byte()? as u32) << (8 * i);
        }
        Ok(f32::from_bits(bits.rotate_left(23)))
    }
    fn string(&mut self) -> Result<String, String> {
        let start = self.p;
        while self.byte()? != 0 {}
        Ok(String::from_utf8_lossy(&self.d[start..self.p - 1]).into_owned())
    }
}

struct Field {
    name: String,
    ty: i64,
    array: bool,
    value: u64,
}

struct Def {
    name: String,
    kind: u8,
    fields: Vec<Field>,
}

struct Decoder<'a> {
    defs: Vec<Def>,
    r: Reader<'a>,
}

impl Decoder<'_> {
    fn value(&mut self, ty: i64, array: bool) -> Result<Value, String> {
        if array {
            let n = self.r.var_uint()? as usize;
            if ty == -2 {
                let start = self.r.p;
                self.r.p += n;
                let slice = self
                    .r
                    .d
                    .get(start..self.r.p)
                    .ok_or("byte array out of range")?;
                return Ok(Value::Bytes(slice.to_vec()));
            }
            let mut v = Vec::with_capacity(n.min(1 << 16));
            for _ in 0..n {
                v.push(self.value(ty, false)?);
            }
            return Ok(Value::Array(v));
        }
        Ok(match ty {
            -1 => Value::Bool(self.r.byte()? != 0),
            -2 => Value::UInt(self.r.byte()? as u64),
            -3 | -7 => Value::Int(self.r.var_int()?),
            -4 | -8 => Value::UInt(self.r.var_uint()?),
            -5 => Value::Float(self.r.float()?),
            -6 => Value::Str(self.r.string()?),
            t if t >= 0 => self.object(t as usize)?,
            t => return Err(format!("unknown kiwi type {t}")),
        })
    }

    fn object(&mut self, idx: usize) -> Result<Value, String> {
        let kind = self.defs[idx].kind;
        match kind {
            0 => {
                let v = self.r.var_uint()?;
                let name = self.defs[idx]
                    .fields
                    .iter()
                    .find(|f| f.value == v)
                    .map(|f| f.name.clone())
                    .unwrap_or_else(|| v.to_string());
                Ok(Value::Enum(name))
            }
            1 => {
                let mut m = BTreeMap::new();
                for i in 0..self.defs[idx].fields.len() {
                    let (name, ty, array) = {
                        let f = &self.defs[idx].fields[i];
                        (f.name.clone(), f.ty, f.array)
                    };
                    m.insert(name, self.value(ty, array)?);
                }
                Ok(Value::Object(m))
            }
            _ => {
                let mut m = BTreeMap::new();
                loop {
                    let key = self.r.var_uint()?;
                    if key == 0 {
                        return Ok(Value::Object(m));
                    }
                    let (name, ty, array) = {
                        let f = self.defs[idx]
                            .fields
                            .iter()
                            .find(|f| f.value == key)
                            .ok_or_else(|| {
                                format!("unknown field {key} in {}", self.defs[idx].name)
                            })?;
                        (f.name.clone(), f.ty, f.array)
                    };
                    m.insert(name, self.value(ty, array)?);
                }
            }
        }
    }
}

fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    if data.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        let mut dec = ruzstd::decoding::StreamingDecoder::new(data).map_err(|e| e.to_string())?;
        dec.read_to_end(&mut out).map_err(|e| e.to_string())?;
    } else {
        flate2::read::DeflateDecoder::new(data)
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Decode a `canvas.fig` payload into its root `Message`.
pub fn decode_canvas(data: &[u8]) -> Result<Value, String> {
    if !data.starts_with(b"fig-kiwi") {
        return Err("not a fig-kiwi file".into());
    }
    let mut chunks = Vec::new();
    let mut off = 12;
    while off + 4 <= data.len() {
        let n = u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as usize;
        chunks.push(&data[off + 4..(off + 4 + n).min(data.len())]);
        off += 4 + n;
    }
    if chunks.len() < 2 {
        return Err("missing kiwi chunks".into());
    }
    let schema = inflate(chunks[0])?;
    let payload = inflate(chunks[1])?;

    let mut sr = Reader { d: &schema, p: 0 };
    let mut defs = Vec::new();
    for _ in 0..sr.var_uint()? {
        let name = sr.string()?;
        let kind = sr.byte()?;
        let mut fields = Vec::new();
        for _ in 0..sr.var_uint()? {
            let fname = sr.string()?;
            let ty = sr.var_int()?;
            let array = sr.byte()? != 0;
            let value = sr.var_uint()?;
            fields.push(Field {
                name: fname,
                ty,
                array,
                value,
            });
        }
        defs.push(Def { name, kind, fields });
    }
    let root = defs
        .iter()
        .position(|d| d.name == "Message")
        .ok_or("no Message definition")?;
    let mut dec = Decoder {
        defs,
        r: Reader { d: &payload, p: 0 },
    };
    dec.object(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_and_floats() {
        let mut r = Reader {
            d: &[0xac, 0x02, 0x03, 0x00, 0x7f, 0x00, 0x00, 0x00],
            p: 0,
        };
        assert_eq!(r.var_uint().unwrap(), 300);
        assert_eq!(r.var_int().unwrap(), -2);
        assert_eq!(r.float().unwrap(), 0.0);
        // 1.0f32 encoded by kiwi: bits rotated right by 23 => 0x7f
        assert_eq!(r.float().unwrap(), 1.0);
    }
}

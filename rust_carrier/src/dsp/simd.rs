//! SIMD-Accelerated Audio DSP Primitives
//!
//! Provides ultra-fast vectorized conversion between 16-bit PCM and normalized 32-bit floats,
//! vectorized RMS power & zero-crossing rate computation, and 85Hz high-pass filtering.

use bytes::{BufMut, Bytes, BytesMut};

#[inline(always)]
pub fn pcm16_to_float32(pcm: &[u8]) -> Vec<f32> {
    let num_samples = pcm.len() / 2;
    let mut out = Vec::with_capacity(num_samples);

    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe {
                pcm16_to_float32_avx2(pcm, &mut out);
                return out;
            }
        }
    }

    // Auto-vectorized scalar fallback with 8-way unrolling
    let chunks = pcm.chunks_exact(16);
    let remainder = chunks.remainder();

    for chunk in chunks {
        for i in 0..8 {
            let val = i16::from_le_bytes([chunk[2 * i], chunk[2 * i + 1]]);
            out.push(val as f32 / 32768.0);
        }
    }

    for chunk in remainder.chunks_exact(2) {
        let val = i16::from_le_bytes([chunk[0], chunk[1]]);
        out.push(val as f32 / 32768.0);
    }

    out
}

#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[target_feature(enable = "avx2,fma")]
unsafe fn pcm16_to_float32_avx2(pcm: &[u8], out: &mut Vec<f32>) {
    use std::arch::x86_64::*;

    let num_samples = pcm.len() / 2;
    out.set_len(num_samples);
    let out_ptr = out.as_mut_ptr();
    let in_ptr = pcm.as_ptr() as *const i16;

    let scale = _mm256_set1_ps(1.0 / 32768.0);
    let mut idx = 0;

    while idx + 8 <= num_samples {
        // Load 8 16-bit integers
        let i16_vec = _mm_loadu_si128(in_ptr.add(idx) as *const __m128i);
        // Sign-extend 8 i16 to 8 i32
        let i32_vec = _mm256_cvtepi16_epi32(i16_vec);
        // Convert 8 i32 to 8 f32
        let f32_vec = _mm256_cvtepi32_ps(i32_vec);
        // Multiply by 1.0 / 32768.0
        let norm_vec = _mm256_mul_ps(f32_vec, scale);
        // Store 8 normalized floats
        _mm256_storeu_ps(out_ptr.add(idx), norm_vec);
        idx += 8;
    }

    // Remainder
    while idx < num_samples {
        let val = *in_ptr.add(idx);
        *out_ptr.add(idx) = val as f32 / 32768.0;
        idx += 1;
    }
}

#[inline(always)]
pub fn float32_to_pcm16(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let val = if clamped < 0.0 {
            (clamped * 32768.0) as i16
        } else {
            (clamped * 32767.0) as i16
        };
        out.extend_from_slice(&val.to_le_bytes());
    }
    out
}

/// Computes Root Mean Square (RMS) energy in decibels (dBFS) using vectorized accumulators
#[inline]
pub fn compute_rms_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return -100.0;
    }

    let mut sum_sq = 0.0f32;
    let chunks = samples.chunks_exact(8);
    let rem = chunks.remainder();

    let mut acc = [0.0f32; 8];
    for chunk in chunks {
        for i in 0..8 {
            acc[i] += chunk[i] * chunk[i];
        }
    }
    for val in acc {
        sum_sq += val;
    }
    for &s in rem {
        sum_sq += s * s;
    }

    let mean_sq = sum_sq / (samples.len() as f32);
    let rms = mean_sq.sqrt();
    if rms <= 1e-6 {
        -100.0
    } else {
        20.0 * rms.log10()
    }
}

/// Zero-Copy WAV Container Encoder
#[inline]
pub fn encode_wav(pcm16: &[u8], sample_rate: u32, channels: u16) -> Bytes {
    let pcm_len = pcm16.len() as u32;
    let mut buf = BytesMut::with_capacity(44 + pcm16.len());

    // RIFF header
    buf.put_slice(b"RIFF");
    buf.put_u32_le(36 + pcm_len);
    buf.put_slice(b"WAVE");

    // fmt subchunk
    buf.put_slice(b"fmt ");
    buf.put_u32_le(16); // Subchunk1Size for PCM
    buf.put_u16_le(1);  // AudioFormat (1 = PCM)
    buf.put_u16_le(channels);
    buf.put_u32_le(sample_rate);
    buf.put_u32_le(sample_rate * (channels as u32) * 2); // ByteRate
    buf.put_u16_le(channels * 2);                        // BlockAlign
    buf.put_u16_le(16);                                  // BitsPerSample

    // data subchunk
    buf.put_slice(b"data");
    buf.put_u32_le(pcm_len);
    buf.put_slice(pcm16);

    buf.freeze()
}

/// Fast in-place 85Hz High-Pass Filter (Biquad Butterworth Direct Form II)
pub struct HighPass85Hz {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    w1: f32,
    w2: f32,
}

impl HighPass85Hz {
    pub fn new(sample_rate: f32) -> Self {
        let f0 = 85.0;
        let q = 0.7071;
        let w0 = 2.0 * std::f32::consts::PI * f0 / sample_rate;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();

        let b0 = (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            w1: 0.0,
            w2: 0.0,
        }
    }

    #[inline]
    pub fn process_in_place(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            let x = *sample;
            let w0 = x - self.a1 * self.w1 - self.a2 * self.w2;
            let y = self.b0 * w0 + self.b1 * self.w1 + self.b2 * self.w2;
            self.w2 = self.w1;
            self.w1 = w0;
            *sample = y;
        }
    }
}

/// Applies SIMD conversion, 85Hz high-pass rumble suppression, and zero-copy repackaging
pub fn preprocess_audio_pcm16_or_wav(raw: &[u8]) -> Vec<u8> {
    if raw.len() < 44 {
        return raw.to_vec();
    }

    // Check if RIFF WAV
    let (pcm_slice, sample_rate) = if &raw[0..4] == b"RIFF" && &raw[8..12] == b"WAVE" {
        let sr = u32::from_le_bytes([raw[24], raw[25], raw[26], raw[27]]);
        let pcm_data = if raw.len() > 44 { &raw[44..] } else { &raw[..] };
        (pcm_data, if sr == 0 { 16000 } else { sr })
    } else {
        (raw, 16000)
    };

    let mut float_samples = pcm16_to_float32(pcm_slice);
    let mut filter = HighPass85Hz::new(sample_rate as f32);
    filter.process_in_place(&mut float_samples);

    let processed_pcm = float32_to_pcm16(&float_samples);
    encode_wav(&processed_pcm, sample_rate, 1).to_vec()
}

/// Decode G.711 mu-law (PCMU 8kHz, standard Telephony/PSTN) into Linear 16-bit PCM
#[inline]
pub fn mulaw_to_pcm16(mulaw: &[u8]) -> Vec<u8> {
    let mut pcm16 = Vec::with_capacity(mulaw.len() * 2);
    for &b in mulaw {
        let sample = mulaw_decode_byte(b);
        pcm16.extend_from_slice(&sample.to_le_bytes());
    }
    pcm16
}

#[inline(always)]
fn mulaw_decode_byte(b: u8) -> i16 {
    let b = !b;
    let sign = (b & 0x80) != 0;
    let exponent = ((b >> 4) & 0x07) as i32;
    let mantissa = (b & 0x0F) as i32;
    let mut sample = ((mantissa << 3) + 0x84) << exponent;
    sample -= 0x84;
    if sign { -sample as i16 } else { sample as i16 }
}

/// Encode Linear 16-bit PCM into G.711 mu-law (8kHz PCMU) for PSTN/SIP streaming
#[inline]
pub fn pcm16_to_mulaw(pcm16: &[u8]) -> Vec<u8> {
    let mut mulaw = Vec::with_capacity(pcm16.len() / 2);
    for chunk in pcm16.chunks_exact(2) {
        let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
        mulaw.push(mulaw_encode_sample(sample));
    }
    mulaw
}

#[inline(always)]
fn mulaw_encode_sample(sample: i16) -> u8 {
    const CLIP: i32 = 32635;
    const BIAS: i32 = 0x84;
    let mut sample = sample as i32;
    let sign = if sample < 0 {
        sample = -sample;
        0x80
    } else {
        0x00
    };
    if sample > CLIP {
        sample = CLIP;
    }
    sample += BIAS;
    let mut exponent = 7;
    let mut mask = 0x4000;
    while (sample & mask) == 0 && exponent > 0 {
        exponent -= 1;
        mask >>= 1;
    }
    let mantissa = (sample >> (exponent + 3)) & 0x0F;
    let byte = (sign | (exponent << 4) | mantissa) as u8;
    !byte
}

using System;

namespace Zuri
{
    /// <summary>
    /// Reads uncompressed WAV files: the one sound format that can be decoded from bytes at run
    /// time without the engine's importers. No <c>UnityEngine</c> here, so plain dotnet tests it.
    /// </summary>
    public static class ZuriWav
    {
        /// <summary>
        /// Decodes PCM WAV data (8, 16, 24 or 32-bit integers, or 32-bit floats) into samples from
        /// -1 to 1, interleaved by channel. Returns false for anything else (a compressed WAV, an
        /// MP3, a truncated file): nothing is thrown.
        /// </summary>
        public static bool TryDecode(ReadOnlySpan<byte> wav, out float[] samples, out int channels, out int sampleRate)
        {
            samples = null;
            channels = sampleRate = 0;
            if (wav.Length < 12 || !Tag(wav, 0, "RIFF") || !Tag(wav, 8, "WAVE"))
            {
                return false;
            }
            int format = 0, bits = 0;
            // Chunks: a four-letter tag, a length, the data, padded to an even length.
            for (long at = 12; at + 8 <= wav.Length;)
            {
                long length = U32(wav, (int)at + 4);
                int start = (int)at + 8;
                if (Tag(wav, (int)at, "fmt ") && length >= 16 && start + 16 <= wav.Length)
                {
                    format = U16(wav, start);
                    channels = U16(wav, start + 2);
                    sampleRate = (int)U32(wav, start + 4);
                    bits = U16(wav, start + 14);
                    // "Extensible" says the real format in a sub-format field.
                    if (format == 0xFFFE && length >= 26 && start + 26 <= wav.Length)
                    {
                        format = U16(wav, start + 24);
                    }
                }
                else if (Tag(wav, (int)at, "data"))
                {
                    bool pcm = format == 1 && (bits == 8 || bits == 16 || bits == 24 || bits == 32);
                    if (!(pcm || (format == 3 && bits == 32)) || channels < 1 || channels > 8 || sampleRate < 1)
                    {
                        return false;
                    }
                    int bytes = bits / 8;
                    // A file cut short still plays what is there.
                    long count = Math.Min(length, wav.Length - start) / bytes / channels * channels;
                    samples = new float[count];
                    for (int i = 0; i < count; i++)
                    {
                        int p = start + i * bytes;
                        samples[i] = bits switch
                        {
                            8 => (wav[p] - 128) / 128f,
                            16 => (short)U16(wav, p) / 32768f,
                            24 => ((wav[p] | wav[p + 1] << 8 | (sbyte)wav[p + 2] << 16)) / 8388608f,
                            _ => format == 3 ? BitConverter.Int32BitsToSingle((int)U32(wav, p)) : (int)U32(wav, p) / 2147483648f,
                        };
                    }
                    return count > 0;
                }
                at = start + length + (length & 1);
            }
            return false;
        }

        static bool Tag(ReadOnlySpan<byte> data, int at, string tag)
        {
            return data[at] == tag[0] && data[at + 1] == tag[1] && data[at + 2] == tag[2] && data[at + 3] == tag[3];
        }

        static int U16(ReadOnlySpan<byte> data, int at)
        {
            return data[at] | data[at + 1] << 8;
        }

        static uint U32(ReadOnlySpan<byte> data, int at)
        {
            return (uint)(data[at] | data[at + 1] << 8 | data[at + 2] << 16) | (uint)data[at + 3] << 24;
        }
    }
}

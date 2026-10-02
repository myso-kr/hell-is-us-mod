// The game's pak AES key, found in its executable the way AESDumpster does
// (.spec/MAP.md §1): eight 32-bit immediate stores to consecutive offsets of one base
// register within a short stretch of code are a key being written; the right candidate
// turns the first block of pakchunk0's encrypted index into an FString mount point
// (`int32 length`, then `../../../`). The key is never written anywhere.

using System.Security.Cryptography;

static class AesKey
{
    public static string Find(string exe, string pak)
    {
        var block = IndexBlock(pak);
        foreach (var key in Candidates(File.ReadAllBytes(exe)))
        {
            using var aes = Aes.Create();
            aes.Key = key;
            var p = aes.DecryptEcb(block, PaddingMode.None);
            var n = BitConverter.ToInt32(p, 0);
            if (n is > 0 and < 512 && p[4] == '.' && p[5] == '.' && p[6] == '/')
                return "0x" + Convert.ToHexString(key);
        }
        throw new InvalidOperationException("no AES key candidate opens the pak index");
    }

    /// The first 32 bytes of the pak's encrypted index.
    static byte[] IndexBlock(string pak)
    {
        using var f = File.OpenRead(pak);
        var tail = new byte[0x400];
        f.Seek(-tail.Length, SeekOrigin.End);
        f.ReadExactly(tail);
        var magic = BitConverter.GetBytes(0x5A6F12E1u);
        var at = -1;
        for (var i = tail.Length - 4; i >= 0; i--)
            if (tail.AsSpan(i, 4).SequenceEqual(magic)) { at = i; break; }
        if (at < 0) throw new InvalidOperationException("no pak footer");
        var offset = BitConverter.ToInt64(tail, at + 8);
        f.Seek(offset, SeekOrigin.Begin);
        var block = new byte[32];
        f.ReadExactly(block);
        return block;
    }

    /// Keys written by eight `mov dword ptr [reg+disp], imm32` in a row (optionally REX.B).
    static IEnumerable<byte[]> Candidates(byte[] d)
    {
        var (lo, hi) = Text(d);
        var stores = new List<(int pos, int reg, int disp, uint imm)>();
        for (var i = lo; i < hi - 8; i++)
        {
            var p = i;
            var rex = d[p] == 0x41;
            if (rex) p++;
            if (d[p] != 0xC7) continue;
            var modrm = d[p + 1];
            int mod = modrm >> 6, rm = modrm & 7, reg = (modrm >> 3) & 7;
            if (reg != 0 || rm == 4 || (mod == 0 && rm == 5) || mod > 1) continue;
            var q = p + 2;
            var disp = 0;
            if (mod == 1) disp = (sbyte)d[q++];
            stores.Add((i, (rex ? 8 : 0) + rm, disp, BitConverter.ToUInt32(d, q)));
        }
        var seen = new HashSet<string>();
        for (var i = 0; i < stores.Count; i++)
        {
            var (start, r0, d0, _) = stores[i];
            var run = new Dictionary<int, uint>();
            for (var j = i; j < Math.Min(i + 24, stores.Count); j++)
            {
                var (pos, r, disp, imm) = stores[j];
                if (pos - start > 160) break;
                var o = disp - d0;
                if (r == r0 && o >= 0 && o < 32 && o % 4 == 0 && !run.ContainsKey(o)) run[o] = imm;
            }
            if (run.Count != 8) continue;
            var key = Enumerable.Range(0, 8).SelectMany(k => BitConverter.GetBytes(run[k * 4])).ToArray();
            if (key.Distinct().Count() < 20 || !seen.Add(Convert.ToHexString(key))) continue;
            yield return key;
        }
    }

    /// The .text section's file range.
    static (int, int) Text(byte[] d)
    {
        var pe = BitConverter.ToInt32(d, 0x3C);
        var n = BitConverter.ToUInt16(d, pe + 6);
        var opt = BitConverter.ToUInt16(d, pe + 20);
        var t = pe + 24 + opt;
        for (var i = 0; i < n; i++)
        {
            var s = t + i * 40;
            if (d[s] == '.' && d[s + 1] == 't' && d[s + 2] == 'e' && d[s + 3] == 'x' && d[s + 4] == 't')
            {
                var size = BitConverter.ToInt32(d, s + 16);
                var raw = BitConverter.ToInt32(d, s + 20);
                return (raw, raw + size);
            }
        }
        throw new InvalidOperationException("no .text section");
    }
}

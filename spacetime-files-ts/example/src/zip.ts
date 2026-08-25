// ZIP archives use STORE mode for small files and require no compression dependency.

const crcTable = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(bytes: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < bytes.length; i++)
    c = crcTable[(c ^ bytes[i]!) & 0xff]! ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function dosDateTime(ms: number | undefined): { time: number; date: number } {
  const d = ms ? new Date(ms) : new Date();
  return {
    time: (d.getHours() << 11) | (d.getMinutes() << 5) | (d.getSeconds() >> 1),
    date:
      (((d.getFullYear() - 1980) & 0x7f) << 9) |
      ((d.getMonth() + 1) << 5) |
      d.getDate(),
  };
}
export interface ZipEntry {
  name: string;
  bytes?: Uint8Array;
  mtimeMs?: number;
  isDir?: boolean; // dir names must end with '/'
}
export function buildZip(entries: ZipEntry[]): Blob {
  const te = new TextEncoder();
  const u16 = (v: number) => new Uint8Array([v & 255, (v >>> 8) & 255]);
  const u32 = (v: number) =>
    new Uint8Array([
      v & 255,
      (v >>> 8) & 255,
      (v >>> 16) & 255,
      (v >>> 24) & 255,
    ]);
  const chunks: Uint8Array[] = [];
  const central: Array<{
    name: Uint8Array;
    crc: number;
    size: number;
    time: number;
    date: number;
    offset: number;
    isDir: boolean;
  }> = [];
  let offset = 0;
  for (const e of entries) {
    const name = te.encode(e.name);
    const data = e.bytes ?? new Uint8Array();
    const crc = e.isDir ? 0 : crc32(data);
    const { time, date } = dosDateTime(e.mtimeMs);
    // Local file header: flag 0x0800 = UTF-8 names, method 0 = store.
    chunks.push(
      u32(0x04034b50),
      u16(20),
      u16(0x0800),
      u16(0),
      u16(time),
      u16(date),
      u32(crc),
      u32(data.length),
      u32(data.length),
      u16(name.length),
      u16(0),
      name,
      data
    );
    central.push({
      name,
      crc,
      size: data.length,
      time,
      date,
      offset,
      isDir: !!e.isDir,
    });
    offset += 30 + name.length + data.length;
  }
  const cdStart = offset;
  let cdSize = 0;
  for (const c of central) {
    chunks.push(
      u32(0x02014b50),
      u16(20),
      u16(20),
      u16(0x0800),
      u16(0),
      u16(c.time),
      u16(c.date),
      u32(c.crc),
      u32(c.size),
      u32(c.size),
      u16(c.name.length),
      u16(0),
      u16(0),
      u16(0),
      u16(0),
      u32(c.isDir ? 0x10 : 0),
      u32(c.offset),
      c.name
    );
    cdSize += 46 + c.name.length;
  }
  chunks.push(
    u32(0x06054b50),
    u16(0),
    u16(0),
    u16(central.length),
    u16(central.length),
    u32(cdSize),
    u32(cdStart),
    u16(0)
  );
  // BlobPart requires an ArrayBuffer-backed byte view under TS 5.7.
  return new Blob(chunks as unknown as BlobPart[], { type: 'application/zip' });
}
// Timestamp in the archive name so repeat downloads don't collide.
export function zipStamp(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
}

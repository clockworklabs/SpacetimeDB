using System;
using System.Collections.Generic;
using System.IO;
using SpacetimeDB;

/// <summary>
/// A stream that reads from an underlying list.
/// 
/// Uses one less allocation than converting to a byte array and building a MemoryStream.
/// </summary>
internal class ListStream : Stream
{
    private List<byte> list;
    private int pos;

    public ListStream(List<byte> data)
    {
        this.list = data;
        this.pos = 0;
    }

    public override bool CanRead => true;

    public override bool CanSeek => true;

    public override bool CanWrite => false;

    public override long Length => list.Count;

    public override long Position { get => pos; set => pos = (int)value; }

    public override void Flush()
    {
        // do nothing
    }

    public override int Read(byte[] buffer, int offset, int count)
    {
        int listPos = pos;
        int listEnd = Math.Min(list.Count, listPos + count);
        int bufPos = offset;
        int bufLength = buffer.Length;
        for (; listPos < listEnd && bufPos < bufLength; listPos++, bufPos++)
        {
            buffer[bufPos] = list[listPos];
        }
        pos = listPos;
        return bufPos - offset;
    }

    public override int Read(Span<byte> buffer)
    {
        int listPos = pos;
        int listLength = list.Count;
        int bufPos = 0;
        int bufLength = buffer.Length;
        for (; listPos < listLength && bufPos < bufLength; listPos++, bufPos++)
        {
            buffer[bufPos] = list[listPos];
        }
        pos = listPos;
        return bufPos;
    }

    public override long Seek(long offset, SeekOrigin origin)
    {
        switch (origin)
        {
            case SeekOrigin.Begin:
                pos = (int)offset;
                break;
            case SeekOrigin.Current:
                pos += (int)offset;
                break;
            case SeekOrigin.End:
                pos = (int)(Length + offset);
                break;
        }
        return pos;
    }

    public override void SetLength(long value)
    {
        throw new System.NotSupportedException();
    }

    public override void Write(byte[] buffer, int offset, int count)
    {
        throw new System.NotSupportedException();
    }
}

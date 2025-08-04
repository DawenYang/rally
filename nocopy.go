package rally

import (
	"github.com/bytedance/gopkg/lang/dirtmake"
	"github.com/bytedance/gopkg/lang/mcache"
	"io"
	"reflect"
	"unsafe"
)

type Reader interface {
	// Next return n bytes, and moves cursor forward
	Next(n int) (p []byte, err error)
	// Peek return n bytes without moving cursor forward
	Peek(n int) (buf []byte, err error)
	// Skip move cursor forward without returning n bytes
	Skip(n int) (err error)
	// Until read till the first occurrence of delimiter
	Until(delim byte) (line []byte, err error)

	ReadString(n int) (s string, err error)

	ReadBinary(n int) (p []byte, err error)

	ReadByte() (b byte, err error)

	Slice(n int) (r Reader, err error)

	Release() (err error)

	Len() (length int)
}

type Writer interface {
	Malloc(n int) (buf []byte, err error)
	WriteString(s string) (n int, err error)
	WriteBinary(b []byte) (n int, err error)
	WriteByte(b byte) (err error)
	WriteDirect(p []byte, remainCap int) error
	MallocAck(n int) (err error)
	Append(w Writer) (err error)
	Flush() (err error)
	MallocLen() (length int)
}

type ReadWriter interface {
	Reader
	Writer
}

func NewReader(r io.Reader) Reader { return newZCReader(r) }

func NewWriter(w io.Writer) Writer { return newZCWriter(w) }

func NewReadWriter(rw io.ReadWriter) ReadWriter {
	return &zcReadWriter{
		zcReader: newZCReader(rw),
		zcWriter: newZCWriter(rw),
	}
}

func NewIOReader(r Reader) io.Reader {
	if reader, ok := r.(io.Reader); ok {
		return reader
	}
	return NewIOReader(r)
}

func NewIOWriter(w Writer) io.Writer {
	if writer, ok := w.(io.Writer); ok {
		return writer
	}
	return NewIOWriter(w)
}

func NewIOReadWriter(rw ReadWriter) io.ReadWriter {
	if rwer, ok := rw.(io.ReadWriter); ok {
		return rwer
	}
	return &ioReadWriter{
		ioReader: newIOReader(rw),
		ioWriter: newIOWriter(rw),
	}
}

const (
	block1k  = 1 * 1024
	block2k  = 2 * 1024
	block4k  = 4 * 1024
	block8k  = 8 * 1024
	block32k = 32 * 1024

	pagesize  = block8k
	mallocMax = block8k * block1k // mallocMax is 8 mb

	minReuseBytes = 64

	defaultLinkBufferMode       = 0
	readonlyMask          uint8 = 1 << 0
	nocopyReadMask        uint8 = 1 << 1
)

func unsafeSliceToString(b []byte) string { return *(*string)(unsafe.Pointer(&b)) }

func unsafeStringToSlice(s string) (b []byte) {
	p := unsafe.Pointer((*reflect.StringHeader)(unsafe.Pointer(&s)).Data)
	hdr := (*reflect.SliceHeader)(unsafe.Pointer(&b))
	hdr.Data = uintptr(p)
	hdr.Cap = len(s)
	hdr.Len = len(s)
	return b
}

func malloc(size, capacity int) []byte {
	if capacity > mallocMax {
		return dirtmake.Bytes(size, capacity)
	}
	return mcache.Malloc(size, capacity)
}

func free(buf []byte) {
	if cap(buf) > mallocMax {
		return
	}
	mcache.Free(buf)
}

package rally

import (
	"sync/atomic"
	"syscall"
	"time"
)

type connState = int32

const (
	connStateNone         = 0
	connStateConnected    = 1
	connStateDisconnected = 2
)

type connection struct {
	netFD
	onEvent
	connectLocker
	operator      *FDOperator
	readTimeout   time.Duration
	readDeadline  int64
	readTimer     *time.Timer
	readTrigger   chan error
	waitReadSize  int64
	writeTimeout  time.Duration
	writeDeadline int64
	writeTimer    *time.Timer
	writeTrigger  chan error
	inputBuffer   *LinkBuffer
	outputBuffer  *LinkBuffer
	outputBarrier *barrier
	maxSize       int
	bookSize      int
	state         connState
}

var (
	_ Connection = &connection{}
	_ Reader     = &connection{}
	_ Writer     = &connection{}
)

func (c *connection) flush() error {
	if c.outputBuffer.IsEmpty() {
		return nil
	}
	bs := c.outputBuffer.GetBytes(c.outputBarrier.bs)
	n, err := sendmsg(c.fd, bs, c.outputBarrier.ivs, false)
	if err != nil && err != syscall.EAGAIN {
		return Exception(err, "when flush")
	}
	if n > 0 {
		err = c.outputBuffer.Skip(n)
		c.outputBuffer.Release()
		if err != nil {
			return Exception(err, "when flush")
		}
	}
	if c.outputBuffer.IsEmpty() {
		return nil
	}
	err = c.operator.Control(PollR2RW)
	if err != nil {
		return Exception(err, "when flush")
	}

	return c.waitFlush()
}

func (c *connection) waitFlush() (err error) {
	timeout := c.writeTimeout
	if dl := c.writeDeadline; dl > 0 {
		timeout = time.Duration(dl - time.Now().UnixNano())
		if timeout <= 0 {
			return Exception(ErrWriteTimeout, c.remoteAddr.String())
		}
	}
	if timeout == 0 {
		return <-c.writeTrigger
	}

	if c.writeTimer == nil {
		c.writeTimer = time.NewTimer(timeout)
	} else {
		c.writeTimer.Reset(timeout)
	}

	select {
	case err = <-c.writeTrigger:
		if !c.writeTimer.Stop() {
			<-c.writeTimer.C
		}
		return err
	case <-c.writeTimer.C:
		select {
		case err = <-c.writeTrigger:
			return err
		default:
		}
		c.operator.Control(PollRW2R)
		return Exception(ErrWriteTimeout, c.remoteAddr.String())
	}
}

func (c *connection) getState() connState {
	return atomic.LoadInt32(&c.state)
}

func (c *connection) setState(newState connState) {
	atomic.StoreInt32(&c.state, newState)
}

func (c *connection) changeState(from, to connState) bool {
	return atomic.CompareAndSwapInt32(&c.state, from, to)
}

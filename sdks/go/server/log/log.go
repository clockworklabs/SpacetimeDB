package log

import "github.com/clockworklabs/SpacetimeDB/sdks/go/server/sys"

// Logger provides logging via SpacetimeDB host console_log.
type Logger interface {
	Error(msg string)
	Warn(msg string)
	Info(msg string)
	Debug(msg string)
	Trace(msg string)
}

// NewLogger creates a Logger that writes to the host console with the given target name.
func NewLogger(target string) Logger {
	return &logger{target: target}
}

type logger struct {
	target string
}

func (l *logger) Error(msg string) {
	sys.ConsoleLog(sys.LogLevelError, l.target, "", 0, msg)
}

func (l *logger) Warn(msg string) {
	sys.ConsoleLog(sys.LogLevelWarn, l.target, "", 0, msg)
}

func (l *logger) Info(msg string) {
	sys.ConsoleLog(sys.LogLevelInfo, l.target, "", 0, msg)
}

func (l *logger) Debug(msg string) {
	sys.ConsoleLog(sys.LogLevelDebug, l.target, "", 0, msg)
}

func (l *logger) Trace(msg string) {
	sys.ConsoleLog(sys.LogLevelTrace, l.target, "", 0, msg)
}

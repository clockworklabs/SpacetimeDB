namespace SpacetimeDB;

using System.Runtime.CompilerServices;
using System.Text;
using SpacetimeDB.Internal;

public static class Log
{
    /// <summary>
    /// Write an error message to module log
    /// </summary>
    /// <param name="message">Message to log</param>
    /// <param name="RESERVED_target"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_filename"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_lineNumber"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    public static void Debug(
        string message,
        [CallerMemberName] string RESERVED_target = "",
        [CallerFilePath] string RESERVED_filename = "",
        [CallerLineNumber] uint RESERVED_lineNumber = 0
    ) =>
        LogInternal(
            message,
            FFI.LogLevel.Debug,
            RESERVED_target,
            RESERVED_filename,
            RESERVED_lineNumber
        );

    /// <summary>
    /// Write a trace message to module log
    /// </summary>
    /// <param name="message">Message to log</param>
    /// <param name="RESERVED_target"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_filename"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_lineNumber"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    public static void Trace(
        string message,
        [CallerMemberName] string RESERVED_target = "",
        [CallerFilePath] string RESERVED_filename = "",
        [CallerLineNumber] uint RESERVED_lineNumber = 0
    ) =>
        LogInternal(
            message,
            FFI.LogLevel.Trace,
            RESERVED_target,
            RESERVED_filename,
            RESERVED_lineNumber
        );

    /// <summary>
    /// Write an info message to module log
    /// </summary>
    /// <param name="message">Message to log</param>
    /// <param name="RESERVED_target"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_filename"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_lineNumber"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    public static void Info(
        string message,
        [CallerMemberName] string RESERVED_target = "",
        [CallerFilePath] string RESERVED_filename = "",
        [CallerLineNumber] uint RESERVED_lineNumber = 0
    ) =>
        LogInternal(
            message,
            FFI.LogLevel.Info,
            RESERVED_target,
            RESERVED_filename,
            RESERVED_lineNumber
        );

    /// <summary>
    /// Write a warning message to module log
    /// </summary>
    /// <param name="message">Message to log</param>
    /// <param name="RESERVED_target"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_filename"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_lineNumber"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    public static void Warn(
        string message,
        [CallerMemberName] string RESERVED_target = "",
        [CallerFilePath] string RESERVED_filename = "",
        [CallerLineNumber] uint RESERVED_lineNumber = 0
    ) =>
        LogInternal(
            message,
            FFI.LogLevel.Warn,
            RESERVED_target,
            RESERVED_filename,
            RESERVED_lineNumber
        );

    /// <summary>
    /// Write an error message to module log
    /// </summary>
    /// <param name="message">Message to log</param>
    /// <param name="RESERVED_target"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_filename"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_lineNumber"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    public static void Error(
        string message,
        [CallerMemberName] string RESERVED_target = "",
        [CallerFilePath] string RESERVED_filename = "",
        [CallerLineNumber] uint RESERVED_lineNumber = 0
    ) =>
        LogInternal(
            message,
            FFI.LogLevel.Error,
            RESERVED_target,
            RESERVED_filename,
            RESERVED_lineNumber
        );

    /// <summary>
    /// Write an exception message to module log
    /// </summary>
    /// <param name="message">Message to log</param>
    /// <param name="RESERVED_target"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_filename"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_lineNumber"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    public static void Exception(
        string message,
        [CallerMemberName] string RESERVED_target = "",
        [CallerFilePath] string RESERVED_filename = "",
        [CallerLineNumber] uint RESERVED_lineNumber = 0
    ) =>
        LogInternal(
            message,
            FFI.LogLevel.Error,
            RESERVED_target,
            RESERVED_filename,
            RESERVED_lineNumber
        );

    /// <summary>
    /// Write an exception message and stacktrace to module log
    /// </summary>
    /// <param name="exception">Exception to log</param>
    /// <param name="RESERVED_target"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_filename"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    /// <param name="RESERVED_lineNumber"><b>!!! DO NOT USE !!!</b> Value for this parameter will be automatically generated at compile time. Providing this parameter could lead to undefined behavior</param>
    public static void Exception(
        Exception exception,
        [CallerMemberName] string RESERVED_target = "",
        [CallerFilePath] string RESERVED_filename = "",
        [CallerLineNumber] uint RESERVED_lineNumber = 0
    ) =>
        LogInternal(
            exception.ToString(),
            FFI.LogLevel.Error,
            RESERVED_target,
            RESERVED_filename,
            RESERVED_lineNumber
        );

    private static void LogInternal(
        string text,
        FFI.LogLevel level,
        string target,
        string filename,
        uint lineNumber
    )
    {
        var target_bytes = Encoding.UTF8.GetBytes(target);
        var filename_bytes = Encoding.UTF8.GetBytes(filename);
        var text_bytes = Encoding.UTF8.GetBytes(text);

        FFI.console_log(
            level,
            target_bytes,
            (uint)target_bytes.Length,
            filename_bytes,
            (uint)filename_bytes.Length,
            lineNumber,
            text_bytes,
            (uint)text_bytes.Length
        );
    }
}

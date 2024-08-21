type LogLevel = 'info' | 'warn' | 'error' | 'debug';

const LogLevelIdentifierIcon = {
  component: '📦',
  info: 'ℹ️',
  warn: '⚠️',
  error: '❌',
  debug: '🐛',
};

const LogStyle = {
  component:
    'color: #fff; background-color: #8D6FDD; padding: 2px 5px; border-radius: 3px;',
  info: 'color: #fff; background-color: #007bff; padding: 2px 5px; border-radius: 3px;',
  warn: 'color: #fff; background-color: #ffc107; padding: 2px 5px; border-radius: 3px;',
  error:
    'color: #fff; background-color: #dc3545; padding: 2px 5px; border-radius: 3px;',
  debug:
    'color: #fff; background-color: #28a745; padding: 2px 5px; border-radius: 3px;',
};

const LogTextStyle = {
  component: 'color: #8D6FDD;',
  info: 'color: #007bff;',
  warn: 'color: #ffc107;',
  error: 'color: #dc3545;',
  debug: 'color: #28a745;',
};

export const stdbLogger = (level: LogLevel, message: any): void => {
  console.log(
    `%c${LogLevelIdentifierIcon[level]} ${level.toUpperCase()}%c ${message}`,
    LogStyle[level],
    LogTextStyle[level]
  );
};

pub enum LogLevel {
	Info,
	Warn,
	Error,
}

pub fn log(level: LogLevel, message: &str) {
	match level {
		LogLevel::Info => eprintln!("INFO: {}", message),
		LogLevel::Warn => eprintln!("WARN: {}", message),
		LogLevel::Error => eprintln!("ERROR: {}", message),
	}
}

// macro_rules! log_info {
// 	($message:expr) => {
// 		log(LogLevel::Info, $message);
// 	};
// }

// macro_rules! log_warn {
// 	($message:expr) => {
// 		log(LogLevel::Warn, $message);
// 	};
// }

// macro_rules! log_error {
// 	($message:expr) => {
// 		log(LogLevel::Error, $message);
// 	};
// }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorDef {
    pub code: &'static str,
    pub message: &'static str,
    pub http_code: u16,
}

/// Fallback code used by the Node `HttpExceptionFilter` for errors that are
/// not mapped to a known `Axxx` code (including request validation failures).
pub const UNKNOWN_ERROR_CODE: &str = "E000";

pub const INTERNAL_SERVER_ERROR: ErrorDef = ErrorDef {
    code: "A001",
    message: "Server error",
    http_code: 500,
};
pub const LOGIN_ERROR: ErrorDef = ErrorDef {
    code: "A002",
    message: "Login error",
    http_code: 500,
};
pub const UNAUTHORIZED: ErrorDef = ErrorDef {
    code: "A003",
    message: "Unauthorized",
    http_code: 401,
};
pub const FORBIDDEN_ROLE_ERROR: ErrorDef = ErrorDef {
    code: "A004",
    message: "Forbidden role error",
    http_code: 403,
};
pub const GET_XRAY_STATS_ERROR: ErrorDef = ErrorDef {
    code: "A009",
    message: "Get Xray stats error",
    http_code: 500,
};
pub const FAILED_TO_GET_SYSTEM_STATS: ErrorDef = ErrorDef {
    code: "A010",
    message: "Failed to get system stats",
    http_code: 500,
};
pub const FAILED_TO_GET_USERS_STATS: ErrorDef = ErrorDef {
    code: "A011",
    message: "Failed to get users stats",
    http_code: 500,
};
pub const FAILED_TO_GET_INBOUND_STATS: ErrorDef = ErrorDef {
    code: "A012",
    message: "Failed to get inbound stats",
    http_code: 500,
};
pub const FAILED_TO_GET_OUTBOUND_STATS: ErrorDef = ErrorDef {
    code: "A013",
    message: "Failed to get outbound stats",
    http_code: 500,
};
pub const FAILED_TO_GET_INBOUND_USERS: ErrorDef = ErrorDef {
    code: "A014",
    message: "Failed to get inbound users",
    http_code: 500,
};
pub const FAILED_TO_GET_INBOUNDS_STATS: ErrorDef = ErrorDef {
    code: "A015",
    message: "Failed to get inbounds stats",
    http_code: 500,
};
pub const FAILED_TO_GET_OUTBOUNDS_STATS: ErrorDef = ErrorDef {
    code: "A016",
    message: "Failed to get outbounds stats",
    http_code: 500,
};
pub const FAILED_TO_GET_COMBINED_STATS: ErrorDef = ErrorDef {
    code: "A017",
    message: "Failed to get combined stats",
    http_code: 500,
};
pub const FAILED_TO_GET_GEOCHECK: ErrorDef = ErrorDef {
    code: "A018",
    message: "Failed to get geocheck report",
    http_code: 500,
};

pub const ALL: &[ErrorDef] = &[
    INTERNAL_SERVER_ERROR,
    LOGIN_ERROR,
    UNAUTHORIZED,
    FORBIDDEN_ROLE_ERROR,
    GET_XRAY_STATS_ERROR,
    FAILED_TO_GET_SYSTEM_STATS,
    FAILED_TO_GET_USERS_STATS,
    FAILED_TO_GET_INBOUND_STATS,
    FAILED_TO_GET_OUTBOUND_STATS,
    FAILED_TO_GET_INBOUND_USERS,
    FAILED_TO_GET_INBOUNDS_STATS,
    FAILED_TO_GET_OUTBOUNDS_STATS,
    FAILED_TO_GET_COMBINED_STATS,
    FAILED_TO_GET_GEOCHECK,
];

pub fn by_code(code: &str) -> Option<ErrorDef> {
    ALL.iter().copied().find(|e| e.code == code)
}

pub const REMNAWAVE_NODE_KNOWN_ERROR: &str = "Remnawave Node Known Error";

pub const XRAY_FAILED_TO_START_CODE: &str = "RN-001";
pub const XRAY_FAILED_TO_START_MESSAGE: &str = "Xray core failed to start";
pub const XRAY_FAILED_TO_START_DOCS: &str =
    "https://docs.rw/docs/guides/common-errors#xml-rpc-fault-spawn-error-xray";

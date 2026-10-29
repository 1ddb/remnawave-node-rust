/// `CAP_NET_ADMIN` bit in the Linux capability mask.
const CAP_NET_ADMIN: u32 = 12;

pub fn has_cap_net_admin() -> bool {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    let Some(eff) = status
        .lines()
        .find_map(|line| line.strip_prefix("CapEff:"))
        .map(str::trim)
    else {
        return false;
    };
    let Ok(mask) = u64::from_str_radix(eff, 16) else {
        return false;
    };
    (mask >> CAP_NET_ADMIN) & 1 == 1
}

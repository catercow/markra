//! Lenient parsing and canonical forms of component input values, keyed by the schema's
//! `format`: `price`, `code`, `link` (table cells) and `search-link`, `review-link` (props).
//! Authoring tools accept what people paste; content is stored in the canonical form only.

use indexmap::IndexMap;

/// Why a value can't be understood. The variants choose the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueError {
    /// Not a price at all ("TBD", "varies", "-7").
    Price,
    /// "7-9", "$7 to $9".
    PriceRange,
    /// "~$13", "about 13", "13+".
    PriceApproximate,
    /// "7.255".
    PriceDecimals,
    /// "7,25".
    PriceComma,
    /// "€7", "7 CAD".
    PriceCurrency,
    /// A promo code with characters other than A–Z, 0–9, `-` and `_`.
    Code,
    /// Not an http(s) address or a site path.
    Link,
    /// Not a CaterCow search address.
    SearchLink,
    /// A search filter this list doesn't know.
    SearchParam { param: String },
    /// A filter value outside the known ones.
    SearchValue {
        param: String,
        value: String,
        suggestion: Option<String>,
        values: Vec<String>,
    },
    /// Not a caterer or package page address.
    ReviewLink,
}

// --- price -------------------------------------------------------------------------

/// `$` and two decimals, with thousands separators: `$7.25`, `$1,200.50`.
pub fn price(input: &str) -> Result<String, ValueError> {
    let text = input.trim();
    let lower = text.to_lowercase();
    if text.starts_with(['~', '≈', '<', '>', '≤', '≥'])
        || text.ends_with('+')
        || [
            "about", "approx", "around", "from ", "starting", "up to", "under ", "over ", "ca.",
            "circa",
        ]
        .iter()
        .any(|word| lower.starts_with(word))
    {
        return Err(ValueError::PriceApproximate);
    }
    let digits_around = |at: usize, len: usize| {
        text[..at]
            .trim_end()
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_digit())
            && text[at + len..]
                .trim_start()
                .trim_start_matches('$')
                .trim_start()
                .starts_with(|c: char| c.is_ascii_digit())
    };
    for dash in ['-', '–', '—'] {
        if let Some(at) = text.find(dash)
            && digits_around(at, dash.len_utf8())
        {
            return Err(ValueError::PriceRange);
        }
    }
    if let Some(at) = lower.find(" to ")
        && digits_around(at, 4)
    {
        return Err(ValueError::PriceRange);
    }
    if text.chars().any(|c| {
        matches!(
            c,
            '€' | '£'
                | '¥'
                | '₹'
                | '₩'
                | '₽'
                | '₪'
                | '₫'
                | '₱'
                | '¢'
                | '฿'
                | '₺'
                | '₴'
                | '₦'
        )
    }) {
        return Err(ValueError::PriceCurrency);
    }

    let mut rest = text;
    let strip_prefix = |rest: &mut &str| {
        let trimmed = rest.trim_start();
        for prefix in ["US$", "$", "USD"] {
            if trimmed.len() >= prefix.len() && trimmed[..prefix.len()].eq_ignore_ascii_case(prefix)
            {
                *rest = trimmed[prefix.len()..].trim_start();
                return true;
            }
        }
        false
    };
    let prefixed = strip_prefix(&mut rest);
    let trimmed = rest.trim_end();
    rest = trimmed;
    let mut dollar = prefixed && !text.trim_start().to_ascii_uppercase().starts_with("USD");
    for suffix in ["USD", "$"] {
        if rest.len() >= suffix.len()
            && rest[rest.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
        {
            if suffix == "$" && dollar {
                break;
            }
            dollar |= suffix == "$";
            rest = rest[..rest.len() - suffix.len()].trim_end();
        }
    }
    // Another currency's code or prefix: "CAD 7", "7 EUR", "C$7".
    let letters: String = rest
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    const CODES: [&str; 18] = [
        "EUR", "GBP", "CAD", "AUD", "JPY", "MXN", "INR", "CNY", "CHF", "NZD", "HKD", "SGD", "SEK",
        "NOK", "DKK", "BRL", "ZAR", "KRW",
    ];
    if (rest.chars().any(|c| c.is_ascii_digit()) && CODES.contains(&letters.as_str()))
        || ["C$", "A$", "NZ$", "HK$", "S$", "MX$"]
            .iter()
            .any(|code| text.starts_with(code))
    {
        return Err(ValueError::PriceCurrency);
    }
    if rest.starts_with('-') {
        return Err(ValueError::Price);
    }

    let (int, frac) = match rest.split_once('.') {
        Some((int, frac)) => (int, Some(frac)),
        None => (rest, None),
    };
    if int.is_empty() && frac.is_none_or(str::is_empty) {
        return Err(ValueError::Price);
    }
    if !int.bytes().all(|b| b.is_ascii_digit() || b == b',')
        || frac.is_some_and(|frac| !frac.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(ValueError::Price);
    }
    let int_digits = if int.contains(',') {
        let groups: Vec<&str> = int.split(',').collect();
        let thousands =
            (1..=3).contains(&groups[0].len()) && groups[1..].iter().all(|group| group.len() == 3);
        if !thousands {
            let comma_decimal = frac.is_none()
                && groups.len() == 2
                && (1..=2).contains(&groups[1].len())
                && !groups[0].is_empty();
            return Err(if comma_decimal {
                ValueError::PriceComma
            } else {
                ValueError::Price
            });
        }
        groups.concat()
    } else {
        int.to_string()
    };
    let frac = frac.unwrap_or("");
    if frac.len() > 2 {
        return Err(ValueError::PriceDecimals);
    }
    let int_digits = int_digits.trim_start_matches('0');
    if int_digits.len() > 15 {
        return Err(ValueError::Price);
    }
    let int_digits = if int_digits.is_empty() {
        "0"
    } else {
        int_digits
    };
    let mut grouped = String::new();
    for (i, digit) in int_digits.chars().enumerate() {
        if i > 0 && (int_digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    Ok(format!("${grouped}.{frac:0<2}"))
}

// --- promo code --------------------------------------------------------------------

/// Trimmed, without internal whitespace, upper case; only A–Z, 0–9, `-` and `_`.
pub fn code(input: &str) -> Result<String, ValueError> {
    let mut text = input.trim();
    if text.len() >= 2 && text.starts_with('`') && text.ends_with('`') {
        text = text[1..text.len() - 1].trim();
    }
    let code: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase();
    if code
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        Ok(code)
    } else {
        Err(ValueError::Code)
    }
}

// --- links -------------------------------------------------------------------------

/// A web address split into its parts; `host` is None for a site path.
#[derive(Debug, Clone, PartialEq)]
struct Address<'a> {
    host: Option<String>,
    /// From the first `/` after the host (or the start of a site path) to before `?` or `#`.
    path: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

fn is_catercow(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let host = host.split(':').next().unwrap_or("");
    host == "catercow.com" || host == "www.catercow.com"
}

/// `https://…`, `http://…`, `www.catercow.com/…`, `catercow.com/…` or `/…`.
fn address(input: &str) -> Option<Address<'_>> {
    let mut text = input.trim();
    if text.len() >= 2 && text.starts_with('<') && text.ends_with('>') {
        text = &text[1..text.len() - 1];
    }
    if text.is_empty() || text.chars().any(char::is_whitespace) {
        return None;
    }
    let (host, rest) = if text.starts_with('/') {
        if text.starts_with("//") {
            return None;
        }
        (None, text)
    } else {
        let after_scheme = ["https://", "http://"].iter().find_map(|scheme| {
            (text.len() >= scheme.len() && text[..scheme.len()].eq_ignore_ascii_case(scheme))
                .then(|| &text[scheme.len()..])
        });
        let after_scheme = match after_scheme {
            Some(rest) => rest,
            // Without a scheme, only a CaterCow address is understood.
            None => {
                let host_end = text.find(['/', '?', '#']).unwrap_or(text.len());
                if !is_catercow(&text[..host_end]) {
                    return None;
                }
                text
            }
        };
        let host_end = after_scheme
            .find(['/', '?', '#'])
            .unwrap_or(after_scheme.len());
        let host = &after_scheme[..host_end];
        if host.is_empty() || !host.contains('.') || host.contains('@') {
            return None;
        }
        (Some(host.to_string()), &after_scheme[host_end..])
    };
    let (rest, fragment) = match rest.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment)),
        None => (rest, None),
    };
    let (path, query) = match rest.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (rest, None),
    };
    Some(Address {
        host,
        path,
        query,
        fragment,
    })
}

/// An http(s) address or a site path; CaterCow addresses become site paths.
pub fn link(input: &str) -> Result<String, ValueError> {
    let text = input.trim();
    let text = if text.len() >= 2 && text.starts_with('<') && text.ends_with('>') {
        &text[1..text.len() - 1]
    } else {
        text
    };
    let address = address(text).ok_or(ValueError::Link)?;
    match &address.host {
        Some(host) if !is_catercow(host) => {
            // Someone else's site: kept as written, with the scheme in lower case.
            let (scheme, rest) = text.split_once("://").ok_or(ValueError::Link)?;
            Ok(format!("{}://{rest}", scheme.to_ascii_lowercase()))
        }
        _ => {
            let mut out = if address.path.is_empty() {
                "/".to_string()
            } else {
                address.path.to_string()
            };
            if let Some(query) = address.query {
                out.push('?');
                out.push_str(query);
            }
            if let Some(fragment) = address.fragment {
                out.push('#');
                out.push_str(fragment);
            }
            Ok(out)
        }
    }
}

/// A search filter the site understands; `values` lists the allowed ones when they are fixed.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub struct SearchParam {
    #[serde(default)]
    pub values: Option<Vec<String>>,
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => match text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                Some(byte) => {
                    out.push(byte);
                    i += 2;
                }
                None => out.push(b'%'),
            },
            byte => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~' | b',' | b':' | b'@' | b'/')
        {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Levenshtein distance over characters, case-insensitive.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current.push(
                (previous[j] + cost)
                    .min(previous[j + 1] + 1)
                    .min(current[j] + 1),
            );
        }
        previous = current;
    }
    previous[b.len()]
}

/// The closest of `options` to `value`, when it is close enough to be a typo.
pub fn did_you_mean(value: &str, options: &[String]) -> Option<String> {
    let normalized = |text: &str| text.to_lowercase().replace([' ', '-'], "_");
    let wanted = normalized(value);
    if let Some(same) = options.iter().find(|option| normalized(option) == wanted) {
        return Some(same.clone());
    }
    let limit = (value.chars().count() / 3).max(2);
    options
        .iter()
        .map(|option| (distance(&wanted, &normalized(option)), option))
        .filter(|(distance, _)| *distance <= limit)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, option)| option.clone())
}

/// A CaterCow search address as a site path with only the known filters, in the schema's
/// order: `/search?package_type=breakfast&presentation_type[]=buffet`. Without `params`
/// (an older schema) every filter is kept, in the order written.
pub fn search_link(
    input: &str,
    params: Option<&IndexMap<String, SearchParam>>,
) -> Result<String, ValueError> {
    let address = address(input).ok_or(ValueError::SearchLink)?;
    if address
        .host
        .as_deref()
        .is_some_and(|host| !is_catercow(host))
    {
        return Err(ValueError::SearchLink);
    }
    if address.path.trim_end_matches('/') != "/search" {
        return Err(ValueError::SearchLink);
    }

    // name → (written as a list, values in order)
    let mut found: IndexMap<String, (bool, Vec<String>)> = IndexMap::new();
    for pair in address
        .query
        .unwrap_or("")
        .split('&')
        .filter(|pair| !pair.is_empty())
    {
        let (raw_name, raw_value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = percent_decode(raw_name);
        let (base, listed) = match name.find('[') {
            Some(open) if name.ends_with(']') => {
                let index = &name[open + 1..name.len() - 1];
                if !index.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(ValueError::SearchParam {
                        param: name.clone(),
                    });
                }
                (name[..open].to_string(), true)
            }
            _ => (name.clone(), false),
        };
        let value = percent_decode(raw_value).trim().to_string();
        let base = match params {
            Some(params) => {
                if params.contains_key(&base) {
                    base
                } else if let Some(known) = params
                    .keys()
                    .find(|known| known.eq_ignore_ascii_case(&base))
                {
                    known.clone()
                } else {
                    return Err(ValueError::SearchParam { param: base });
                }
            }
            None => base,
        };
        let entry = found.entry(base).or_insert((false, Vec::new()));
        entry.0 |= listed;
        if !value.is_empty() {
            entry.1.push(value);
        }
    }

    let mut parts = Vec::new();
    let mut push = |name: &str, (listed, values): &(bool, Vec<String>)| -> Result<(), ValueError> {
        let allowed = params
            .and_then(|params| params.get(name))
            .and_then(|param| param.values.as_ref());
        let mut canonical = Vec::new();
        for value in values {
            let value = match allowed {
                None => value.clone(),
                Some(allowed) if allowed.contains(value) => value.clone(),
                Some(allowed) => match allowed
                    .iter()
                    .find(|option| option.eq_ignore_ascii_case(value))
                {
                    Some(option) => option.clone(),
                    None => {
                        return Err(ValueError::SearchValue {
                            param: name.to_string(),
                            value: value.clone(),
                            suggestion: did_you_mean(value, allowed),
                            values: allowed.clone(),
                        });
                    }
                },
            };
            canonical.push(value);
        }
        let list = *listed || canonical.len() > 1;
        for value in canonical {
            let key = if list {
                format!("{}[]", percent_encode(name))
            } else {
                percent_encode(name)
            };
            parts.push(format!("{key}={}", percent_encode(&value)));
        }
        Ok(())
    };
    match params {
        Some(params) => {
            for name in params.keys() {
                if let Some(entry) = found.get(name) {
                    push(name, entry)?;
                }
            }
        }
        None => {
            for (name, entry) in &found {
                push(name, entry)?;
            }
        }
    }
    Ok(if parts.is_empty() {
        "/search".to_string()
    } else {
        format!("/search?{}", parts.join("&"))
    })
}

/// A caterer page (`/catering/<id>-<slug>`) or package page (`/packages/<id>-<slug>`) as a
/// site path, without query or fragment. The id is what the component reads.
pub fn review_link(input: &str) -> Result<String, ValueError> {
    let address = address(input).ok_or(ValueError::ReviewLink)?;
    if address
        .host
        .as_deref()
        .is_some_and(|host| !is_catercow(host))
    {
        return Err(ValueError::ReviewLink);
    }
    let path = address.path.trim_end_matches('/');
    let segments: Vec<&str> = path.split('/').collect();
    let [_, kind @ ("catering" | "packages"), page] = segments.as_slice() else {
        return Err(ValueError::ReviewLink);
    };
    let id_len = page.bytes().take_while(u8::is_ascii_digit).count();
    let after = &page[id_len..];
    if id_len == 0 || !(after.is_empty() || after.starts_with('-')) {
        return Err(ValueError::ReviewLink);
    }
    Ok(format!("/{kind}/{page}"))
}

/// The id a review link names, and whether it is a package (else a caterer page).
pub fn review_link_id(canonical: &str) -> Option<(bool, u64)> {
    let rest = canonical
        .strip_prefix("/packages/")
        .map(|rest| (true, rest))
        .or_else(|| {
            canonical
                .strip_prefix("/catering/")
                .map(|rest| (false, rest))
        })?;
    let digits: String = rest.1.chars().take_while(char::is_ascii_digit).collect();
    Some((rest.0, digits.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_are_read_leniently() {
        for (input, expected) in [
            ("7.25", "$7.25"),
            ("$7.25", "$7.25"),
            (" $ 7.25 ", "$7.25"),
            ("7.25$", "$7.25"),
            ("7.25 USD", "$7.25"),
            ("usd 7.25", "$7.25"),
            ("USD7", "$7.00"),
            ("$12 USD", "$12.00"),
            ("12$ USD", "$12.00"),
            ("US$ 9", "$9.00"),
            ("7", "$7.00"),
            ("7.5", "$7.50"),
            ("7.", "$7.00"),
            (".5", "$0.50"),
            ("0", "$0.00"),
            ("07.25", "$7.25"),
            ("1,200.50", "$1,200.50"),
            ("1200.5", "$1,200.50"),
            ("$1,200", "$1,200.00"),
            ("12,000", "$12,000.00"),
            ("1234567", "$1,234,567.00"),
            ("$1,234,567.8", "$1,234,567.80"),
        ] {
            assert_eq!(price(input), Ok(expected.to_string()), "{input}");
        }
    }

    #[test]
    fn canonical_prices_stay_as_they_are() {
        for canonical in [
            "$7.25",
            "$7.00",
            "$1,200.50",
            "$0.00",
            "$999.99",
            "$1,000.00",
        ] {
            assert_eq!(price(canonical).as_deref(), Ok(canonical));
        }
    }

    #[test]
    fn prices_that_cannot_be_read() {
        use ValueError::*;
        for (input, error) in [
            ("", Price),
            ("   ", Price),
            ("$", Price),
            ("TBD", Price),
            ("varies", Price),
            ("Market price", Price),
            ("-7", Price),
            ("$-7", Price),
            ("7-9", PriceRange),
            ("7–9", PriceRange),
            ("$7 - $9", PriceRange),
            ("7 to 9", PriceRange),
            ("~$13", PriceApproximate),
            ("about $13", PriceApproximate),
            ("13+", PriceApproximate),
            ("7,25", PriceComma),
            ("1,2345", Price),
            ("12,00,000", Price),
            ("7.255", PriceDecimals),
            ("7.250", PriceDecimals),
            ("€7", PriceCurrency),
            ("£7.25", PriceCurrency),
            ("7 EUR", PriceCurrency),
            ("CAD 7", PriceCurrency),
            ("C$7", PriceCurrency),
            ("$7/person", Price),
            ("7.2.5", Price),
            ("$$7", Price),
            ("$7$", Price),
            ("USD USD 7", Price),
        ] {
            assert_eq!(price(input), Err(error.clone()), "{input:?}");
        }
    }

    #[test]
    fn codes_are_upper_case_without_spaces() {
        assert_eq!(code(" save 10 ").as_deref(), Ok("SAVE10"));
        assert_eq!(
            code("100off-wegmans_nyc").as_deref(),
            Ok("100OFF-WEGMANS_NYC")
        );
        assert_eq!(code("`abc`").as_deref(), Ok("ABC"));
        assert_eq!(code("SAVE10").as_deref(), Ok("SAVE10"));
        assert_eq!(code("save!10"), Err(ValueError::Code));
        assert_eq!(code("café"), Err(ValueError::Code));
    }

    #[test]
    fn links_become_site_paths_on_catercow() {
        for (input, expected) in [
            (
                "https://www.catercow.com/explore/boston/catering",
                "/explore/boston/catering",
            ),
            (
                "http://catercow.com/explore/boston/catering?x=1#top",
                "/explore/boston/catering?x=1#top",
            ),
            ("HTTPS://WWW.CATERCOW.COM", "/"),
            ("www.catercow.com/blog", "/blog"),
            ("catercow.com/blog/", "/blog/"),
            ("<https://www.catercow.com/a>", "/a"),
            ("/explore/austin/catering", "/explore/austin/catering"),
            ("https://example.com/Menu", "https://example.com/Menu"),
            ("HTTPS://example.com/x", "https://example.com/x"),
        ] {
            assert_eq!(link(input).as_deref(), Ok(expected), "{input}");
        }
        for bad in [
            "",
            "example.com/x",
            "www.example.com",
            "ftp://x.com",
            "//x.com/a",
            "javascript:alert(1)",
            "/a b",
            "mailto:a@b.co",
        ] {
            assert_eq!(link(bad), Err(ValueError::Link), "{bad}");
        }
    }

    fn params() -> IndexMap<String, SearchParam> {
        serde_json::from_str(
            r#"{
              "package_type": { "values": ["breakfast", "lunch_or_dinner", "appetizer_snack", "dessert"] },
              "presentation_type": { "values": ["individual", "buffet"] },
              "package_category": { "values": null },
              "dietary_tags": { "values": ["vegan", "vegetarian", "gluten_free"] },
              "formatted_address": {}
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn search_links_keep_known_filters_in_schema_order() {
        let params = params();
        for (input, expected) in [
            ("https://www.catercow.com/search", "/search"),
            ("/search/", "/search"),
            (
                "https://www.catercow.com/search?presentation_type=buffet&package_type=breakfast",
                "/search?package_type=breakfast&presentation_type=buffet",
            ),
            (
                "catercow.com/search?package_type=Breakfast",
                "/search?package_type=breakfast",
            ),
            (
                "/search?package_type=&package_category=lunch_mexican",
                "/search?package_category=lunch_mexican",
            ),
            (
                "/search?dietary_tags%5B%5D=vegan&dietary_tags%5B%5D=gluten_free",
                "/search?dietary_tags[]=vegan&dietary_tags[]=gluten_free",
            ),
            (
                "/search?dietary_tags[0]=vegan",
                "/search?dietary_tags[]=vegan",
            ),
            (
                "/search?dietary_tags=vegan&dietary_tags=vegetarian",
                "/search?dietary_tags[]=vegan&dietary_tags[]=vegetarian",
            ),
            (
                "/search?formatted_address=Austin%2C+TX&Package_Type=dessert#results",
                "/search?package_type=dessert&formatted_address=Austin,%20TX",
            ),
        ] {
            assert_eq!(
                search_link(input, Some(&params)).as_deref(),
                Ok(expected),
                "{input}"
            );
            // Canonical stays canonical.
            assert_eq!(
                search_link(expected, Some(&params)).as_deref(),
                Ok(expected),
                "{expected}"
            );
        }
    }

    #[test]
    fn search_links_report_unknown_filters_and_values() {
        let params = params();
        assert_eq!(
            search_link("https://example.com/search", Some(&params)),
            Err(ValueError::SearchLink)
        );
        assert_eq!(
            search_link("/catering/12-x", Some(&params)),
            Err(ValueError::SearchLink)
        );
        assert_eq!(
            search_link("not a url", Some(&params)),
            Err(ValueError::SearchLink)
        );
        assert_eq!(
            search_link("/search?utm_source=x", Some(&params)),
            Err(ValueError::SearchParam {
                param: "utm_source".into()
            })
        );
        match search_link("/search?package_type=brekfast", Some(&params)) {
            Err(ValueError::SearchValue {
                param,
                value,
                suggestion,
                ..
            }) => {
                assert_eq!(
                    (param.as_str(), value.as_str(), suggestion.as_deref()),
                    ("package_type", "brekfast", Some("breakfast"))
                );
            }
            other => panic!("{other:?}"),
        }
        match search_link("/search?presentation_type=family", Some(&params)) {
            Err(ValueError::SearchValue {
                suggestion: None,
                values,
                ..
            }) => assert_eq!(values, ["individual", "buffet"]),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            did_you_mean(
                "lunch dinner",
                &params["package_type"].values.clone().unwrap()
            ),
            Some("lunch_or_dinner".into())
        );
        // Without a list of filters, everything is kept in the order written.
        assert_eq!(
            search_link("/search?b=2&a=1", None).as_deref(),
            Ok("/search?b=2&a=1")
        );
    }

    #[test]
    fn review_links_name_a_caterer_or_package() {
        for (input, expected) in [
            (
                "https://www.catercow.com/catering/1234-marianos-tacos",
                "/catering/1234-marianos-tacos",
            ),
            (
                "/catering/1234-marianos-tacos/?tab=reviews#x",
                "/catering/1234-marianos-tacos",
            ),
            ("catercow.com/packages/88-taco-bar", "/packages/88-taco-bar"),
            ("/packages/88", "/packages/88"),
        ] {
            assert_eq!(review_link(input).as_deref(), Ok(expected), "{input}");
        }
        assert_eq!(review_link_id("/packages/88-taco-bar"), Some((true, 88)));
        assert_eq!(review_link_id("/catering/1234-x"), Some((false, 1234)));
        for bad in [
            "/catering/marianos",
            "/brands/marianos",
            "https://example.com/catering/12-x",
            "/catering/12x",
            "/catering/12/menu",
            "",
        ] {
            assert_eq!(review_link(bad), Err(ValueError::ReviewLink), "{bad}");
        }
    }
}

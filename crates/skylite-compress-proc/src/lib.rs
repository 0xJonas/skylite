use std::env;

use proc_macro2::{Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};
use skylite_compress::{compress, CompressionMethods, CompressionReport};

extern crate proc_macro;

enum ReportMode {
    None,
    Normal,
    Full
}

fn get_report_mode() -> ReportMode {
    match env::var("SKYLITE_COMPRESSION_REPORT") {
        Ok(s) => {
            match s.as_str() {
                "none" => ReportMode::None,
                "normal" => ReportMode::Normal,
                "full" => ReportMode::Full,
                _ => ReportMode::Normal
            }
        },
        _ => ReportMode::None
    }
}

fn calc_percent_reduction(initial_size: usize, new_size: usize) -> f32 {
    100.0 - (initial_size - new_size) as f32 / initial_size as f32 * 100.0
}

fn print_compression_report(data_name: &str, initial_size: usize, reports: &[CompressionReport]) {
    match get_report_mode() {
        ReportMode::Normal => {
            let final_size = reports.last().unwrap().compressed_size;
            println!("{}: from {} to {} (reduction of {:.2}%)", data_name, initial_size, final_size, calc_percent_reduction(initial_size, final_size));
        },
        ReportMode::Full => {
            let mut prev_size = initial_size;
            println!("{}:", data_name);
            for report in reports {
                let method_name = match report.method {
                    CompressionMethods::Raw => "Raw data",
                    CompressionMethods::LZ77 => "Lempel-Ziv 77",
                    CompressionMethods::RC => "Range Coding"
                };
                if report.skipped {
                    println!("\t{}: (skipped)", method_name);
                } else {
                    println!("\t{}: from {} to {} (reduction of {:.2}%)", method_name, prev_size, report.compressed_size, calc_percent_reduction(prev_size, report.compressed_size));
                }
                prev_size = report.compressed_size;
            }
        },
        ReportMode::None => {}
    }
}

#[derive(Debug)]
enum ProcError {
    Syntax(String),
    Data(String)
}

impl std::fmt::Display for ProcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax(str) => write!(f, "Syntax Error: {}", str),
            Self::Data(str) => write!(f, "Data Error: {}", str)
        }
    }
}

impl Into<TokenStream> for ProcError {
    fn into(self) -> TokenStream {
        vec![
            TokenTree::Ident(Ident::new("compile_error!", Span::call_site())),
            TokenTree::Group(Group::new(proc_macro2::Delimiter::Parenthesis, TokenTree::Literal(Literal::string(&self.to_string())).into()))
        ].into_iter().collect()
    }
}

fn generate_tokens(data_name: &str, data: &[u8], methods: &[CompressionMethods]) -> TokenStream {
    let (compressed_data, reports) = compress(data, methods);
    print_compression_report(data_name, data.len(), &reports);
    TokenTree::Group(Group::new(
        proc_macro2::Delimiter::Bracket,
        TokenStream::from_iter(
            compressed_data.iter().flat_map(|v| [
                TokenTree::Literal(Literal::u8_suffixed(*v)),
                TokenTree::Punct(Punct::new(',', Spacing::Alone))
            ])
        )
    )).into()
}

/// Iterator over a `TokenStream` that ensures that the `TokenStream`
/// represents a comma-delimited list. If this is not the case, yields
/// a `Result::Err(ProcError)` at the first instance which could not be parsed.
struct DelimitedListIterator {
    stream: Box<dyn Iterator<Item = TokenTree>>
}

impl From<TokenStream> for DelimitedListIterator {
    fn from(value: TokenStream) -> Self {
        DelimitedListIterator {
            stream: Box::new(value.into_iter())
        }
    }
}

impl Iterator for DelimitedListIterator {
    type Item = Result<TokenTree, ProcError>;

    fn next(&mut self) -> Option<Self::Item> {
        let out = self.stream.next()?;
        match self.stream.next() {
            None => Some(Ok(out)),
            Some(TokenTree::Punct(p)) if p.as_char() == ',' => Some(Ok(out)),
            _ => Some(Err(ProcError::Syntax("Expected ','".to_owned())))
        }
    }
}

fn literals_to_data(iter: DelimitedListIterator) -> Result<Vec<u8>, ProcError> {
    let maybe_u8_list: Vec<Result<u8, ProcError>> = iter
        .map(|l| if let TokenTree::Literal(l) = l? {
            Ok(l)
        } else {
            Err(ProcError::Syntax("Expected u8 literal".to_owned()))
        })
        .map(|l| l?
            .to_string()
            .parse::<u8>()
            .map_err(|err| ProcError::Data(err.to_string())))
        .collect();

    let mut out: Vec<u8> = Vec::with_capacity(maybe_u8_list.len());
    for m in maybe_u8_list {
        match m {
            Ok(v) => out.push(v),
            Err(err) => return Err(err)
        }
    }

    Ok(out)
}

fn literals_to_methods(iter: DelimitedListIterator) -> Result<Vec<CompressionMethods>, ProcError> {
    let maybe_method_list: Vec<Result<CompressionMethods, ProcError>> = iter
        .map(|l| if let TokenTree::Ident(i) = l? {
            Ok(i)
        } else {
            Err(ProcError::Syntax("Expected compression methods identifier".to_owned()))
        })
        .map(|l| match l?.to_string().as_str() {
            "lz77" => Ok(CompressionMethods::LZ77),
            "rc" => Ok(CompressionMethods::RC),
            s @ _ => Err(ProcError::Data(format!("Unknown compression method {}", s)))
        })
        .collect();

    let mut out: Vec<CompressionMethods> = Vec::with_capacity(maybe_method_list.len());
    for m in maybe_method_list {
        match m {
            Ok(method) => out.push(method),
            Err(err) => return Err(err)
        }
    }

    Ok(out)
}

fn compressed2(stream: TokenStream) -> TokenStream {
    let mut params: DelimitedListIterator = stream.into();

    let data_iter: DelimitedListIterator = match params.next() {
        Some(Ok(TokenTree::Group(g))) => g.stream().into(),
        Some(Err(err)) => return err.into(),
        _ => return ProcError::Syntax("Expected data".to_owned()).into()
    };

    let data = match literals_to_data(data_iter) {
        Ok(d) => d,
        Err(err) => return err.into()
    };

    let method_iter: DelimitedListIterator = match params.next() {
        Some(Ok(TokenTree::Group(g))) => g.stream().into(),
        Some(Err(err)) => return err.into(),
        _ => return ProcError::Syntax("Expected list of compression methods".to_owned()).into()
    };

    let methods = match literals_to_methods(method_iter) {
        Ok(m) => m,
        Err(err) => return err.into()
    };

    let data_name = match params.next() {
        Some(Ok(TokenTree::Literal(token))) => token.to_string(),
        None => "<anonymous>".to_owned(),
        Some(Err(err)) => return err.into(),
        _ => return ProcError::Syntax("Data name must be a string lteral".to_owned()).into()
    };

    generate_tokens(&data_name, &data, &methods)
}

/// Compresses the data passed to it using the given compression methods and
/// returns an array expression (`[ <data> ]`).
///
/// Syntax: `compressed!([ <data> ], [ <methods> ], <name>)`.
///
/// `<data>` must be a comma-delimited list of u8 literals. `<methods>` must be a comma-delimited list
/// contains any of the following identifiers:
/// - `lz77`: Lempel-Ziv 77 compression
/// - `rc`: Range Coding compression.
///
/// The compression methods are applied in the given order, but some may be skipped, if it is found
/// that the size was not reduced after compression.
///
/// ## Example:
///
/// ```rust
/// # #[macro_use] extern crate skylite_compress_proc;
/// # use skylite_compress_proc::compressed;
/// const MY_DATA: &[u8] = &compressed!([0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3], [lz77, rc], "test");
/// ```
#[proc_macro]
pub fn compressed(stream: proc_macro::TokenStream) -> proc_macro::TokenStream {
    compressed2(stream.into()).into()
}

#[cfg(test)]
extern crate quote;

#[cfg(test)]
mod tests {
    use crate::compressed2;
    use crate::quote::quote;

    #[test]
    fn compression_success() {
        let res = compressed2(quote!( [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3], [lz77, rc], "test" ));
        assert_eq!(res.to_string(), "[99u8 , 234u8 , 53u8 , 29u8 , 44u8 , 57u8 , 90u8 , 89u8 , 54u8 , 6u8 , 88u8 , 96u8 ,]");
    }
}

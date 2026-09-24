//! Reads a PDF with the extractor, for checking it against a document by hand.
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: extract_pdf <file.pdf>");
    let bytes = std::fs::read(&path).unwrap();
    match pwr_tools::document::extract(&bytes) {
        Ok(extraction) => {
            println!(
                "--- method: {} | pages: {:?} | links: {:?}",
                extraction.method, extraction.pages, extraction.links
            );
            println!("{}", extraction.text);
        }
        Err(failure) => println!("REFUSED: {failure}"),
    }
}

use html2typst::parse_html;

fn main() {
    let s = r#"
    <img
      src="data:image/png;base64,AAAA"
      alt="Red dot PNG"
    />
    "#;

    println!("{}", parse_html(s));
}

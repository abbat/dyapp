//! Keep displayed summaries finite without splitting extended graphemes.
use unicode_segmentation::UnicodeSegmentation;

const GRAPHEMES: usize = 80;
const BYTES: usize = 512;
const MARKER: &str = "…";

pub(super) fn bounded(input: &str) -> String {
    let mut output = String::new();
    let mut count = 0;
    let mut space = false;
    for grapheme in input.graphemes(true) {
        if grapheme
            .chars()
            .all(|value| value.is_whitespace() || value.is_control())
        {
            space = !output.is_empty();
            continue;
        }
        let padding = usize::from(space);
        if count + padding + 1 > GRAPHEMES || output.len() + padding + grapheme.len() > BYTES {
            return marked(output);
        }
        if space {
            output.push(' ');
        }
        output.push_str(grapheme);
        count += padding + 1;
        space = false;
    }
    output
}

fn marked(mut output: String) -> String {
    while output.graphemes(true).count() >= GRAPHEMES || output.len() + MARKER.len() > BYTES {
        let Some((start, _)) = output.grapheme_indices(true).next_back() else {
            break;
        };
        output.truncate(start);
    }
    output.push_str(MARKER);
    output
}

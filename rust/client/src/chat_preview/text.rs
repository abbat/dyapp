//! Keep displayed summaries finite without splitting extended graphemes.
use unicode_segmentation::UnicodeSegmentation;

const GRAPHEMES: usize = 80;
const BYTES: usize = 512;
const MARKER: &str = "…";

pub(super) fn bounded(input: &str) -> String {
    let mut output = String::new();
    let mut count: usize = 0;
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
        let next = count.saturating_add(padding).saturating_add(1);
        let size = output.len().saturating_add(padding);
        if next > GRAPHEMES || size.saturating_add(grapheme.len()) > BYTES {
            return marked(output);
        }
        if space {
            output.push(' ');
        }
        output.push_str(grapheme);
        count = next;
        space = false;
    }
    output
}

fn marked(mut output: String) -> String {
    while output.graphemes(true).count() >= GRAPHEMES
        || output.len().saturating_add(MARKER.len()) > BYTES
    {
        let Some((start, _)) = output.grapheme_indices(true).next_back() else {
            break;
        };
        output.truncate(start);
    }
    output.push_str(MARKER);
    output
}

match p { 0..=9 | -1 => 1, Point { x, .. } => x, _ => 0 }

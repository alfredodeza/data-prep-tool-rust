use std::collections::HashMap;

/// Per-column running statistics collected in a single pass over the CSV.
pub struct ColumnStats {
    pub name: String,
    pub count: usize,
    pub missing: usize,
    pub numeric_count: usize,
    pub integer_count: usize,
    pub sum: f64,
    pub sum_sq: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub min_len: Option<usize>,
    pub max_len: Option<usize>,
    pub value_counts: HashMap<String, usize>,
    /// Cap on how many distinct values we track exactly before we stop
    /// growing the map (still counted, just no longer distinguished).
    distinct_cap: usize,
    pub distinct_overflow: bool,
}

impl ColumnStats {
    pub fn new(name: String) -> Self {
        ColumnStats {
            name,
            count: 0,
            missing: 0,
            numeric_count: 0,
            integer_count: 0,
            sum: 0.0,
            sum_sq: 0.0,
            min: None,
            max: None,
            min_len: None,
            max_len: None,
            value_counts: HashMap::new(),
            distinct_cap: 10_000,
            distinct_overflow: false,
        }
    }

    pub fn observe(&mut self, raw: &str) {
        self.count += 1;
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            self.missing += 1;
            return;
        }

        let len = trimmed.chars().count();
        self.min_len = Some(self.min_len.map_or(len, |m| m.min(len)));
        self.max_len = Some(self.max_len.map_or(len, |m| m.max(len)));

        let normalized = trimmed.replace('_', "");
        if let Ok(n) = normalized.parse::<f64>() {
            if n.is_finite() {
                self.numeric_count += 1;
                self.sum += n;
                self.sum_sq += n * n;
                self.min = Some(self.min.map_or(n, |m| m.min(n)));
                self.max = Some(self.max.map_or(n, |m| m.max(n)));

                // Whole numbers written without a decimal point or exponent are
                // treated as integers; anything else (e.g. "1.0", "1e3") is a float.
                let looks_integer = !normalized.contains('.') && !normalized.contains(['e', 'E']);
                if looks_integer && normalized.parse::<i64>().is_ok() {
                    self.integer_count += 1;
                }
            }
        }

        if self.value_counts.len() < self.distinct_cap || self.value_counts.contains_key(trimmed) {
            *self.value_counts.entry(trimmed.to_string()).or_insert(0) += 1;
        } else {
            self.distinct_overflow = true;
        }
    }

    pub fn present(&self) -> usize {
        self.count - self.missing
    }

    /// A column is treated as numeric when every non-missing value parsed as a number.
    pub fn is_numeric(&self) -> bool {
        self.present() > 0 && self.numeric_count == self.present()
    }

    /// A numeric column is treated as an integer column when every numeric value
    /// parsed as a whole number (no decimal point or exponent).
    pub fn is_integer(&self) -> bool {
        self.numeric_count > 0 && self.integer_count == self.numeric_count
    }

    pub fn mean(&self) -> Option<f64> {
        if self.numeric_count == 0 {
            None
        } else {
            Some(self.sum / self.numeric_count as f64)
        }
    }

    pub fn stddev(&self) -> Option<f64> {
        if self.numeric_count < 2 {
            return None;
        }
        let n = self.numeric_count as f64;
        let mean = self.sum / n;
        let variance = (self.sum_sq / n) - (mean * mean);
        Some(variance.max(0.0).sqrt())
    }

    pub fn distinct_count(&self) -> usize {
        self.value_counts.len()
    }

    /// Top-k most frequent values, most frequent first.
    pub fn top_values(&self, k: usize) -> Vec<(&str, usize)> {
        let mut items: Vec<(&str, usize)> = self
            .value_counts
            .iter()
            .map(|(v, c)| (v.as_str(), *c))
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        items.truncate(k);
        items
    }
}

pub struct TableSummary {
    pub file: String,
    pub rows: usize,
    pub columns: Vec<ColumnStats>,
}

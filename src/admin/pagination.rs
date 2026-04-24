/*  This file is part of riposte-social
 *  Copyright (C) 2026 Grant DeFayette
 *
 *  riposte-social is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, version 3 of the License (GPL-3.0-only).
 *
 *  riposte-social is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with riposte-social.  If not, see <https://www.gnu.org/licenses/gpl-3.0.html>.
 */
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_per_page")]
    pub per_page: u64,
}

fn default_page() -> u64 {
    1
}

fn default_per_page() -> u64 {
    100
}

impl PaginationParams {
    /// Validate and normalize pagination parameters
    /// - page: minimum 1
    /// - per_page: minimum 1, maximum 500
    pub fn validate(&self) -> ValidatedPagination {
        ValidatedPagination {
            page: self.page.max(1),
            per_page: self.per_page.clamp(1, 500),
        }
    }
}

pub struct ValidatedPagination {
    pub page: u64,
    pub per_page: u64,
}

#[derive(Serialize)]
pub struct Paginated<T> {
    pub data: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub total_pages: u64,
}

impl<T> Paginated<T> {
    pub fn new(data: Vec<T>, total: u64, page: u64, per_page: u64, total_pages: u64) -> Self {
        Self {
            data,
            total,
            page,
            per_page,
            total_pages,
        }
    }
}

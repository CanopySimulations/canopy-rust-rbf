use wasm_bindgen::prelude::*;
use nalgebra::{DMatrix, DVector};

#[wasm_bindgen]
extern {
    pub fn alert(s: &str);
}

static mut SCATTER: Option<Scatter> = None;

#[wasm_bindgen]
pub fn get_interpolant(xsvalue: JsValue, ysvalue: JsValue) {
    let xs_temp: Vec<Vec<f64>> = serde_wasm_bindgen::from_value(xsvalue).unwrap();
    let ys_temp: Vec<Vec<f64>> = serde_wasm_bindgen::from_value(ysvalue).unwrap();
    let mut xs: Vec<DVector<f64>> = Vec::new();
    let mut ys: Vec<DVector<f64>> = Vec::new();
    for i in 0..xs_temp.len(){
        xs.push(DVector::from_vec(xs_temp[i].clone()));
    }
    for i in 0..ys_temp.len(){
        ys.push(DVector::from_vec(ys_temp[i].clone()));
    }
    unsafe { 
        SCATTER = Some(Scatter::create(xs, ys, Basis::PolyHarmonic(1), 0));
    }
}

#[wasm_bindgen]
pub fn get_values(values: JsValue) -> JsValue {
    let values2: Vec<f64> = serde_wasm_bindgen::from_value(values).unwrap();
    let computed_values = unsafe {SCATTER.as_mut().expect("not initialised").eval(DVector::from_vec(values2))};
    serde_wasm_bindgen::to_value(computed_values.data.as_vec()).unwrap()
}

pub enum Basis {
    PolyHarmonic(i32),
    Gaussian(f64),
    MultiQuadric(f64),
    InverseMultiQuadric(f64),
}

pub struct Scatter {
    // Note: could make basis a type-level parameter
    basis: Basis,
    // TODO(explore): use matrix & slicing instead (fewer allocs).
    // An array of n vectors each of size m.
    centers: Vec<DVector<f64>>,
    // An m x n' matrix, where n' is the number of basis functions (including polynomial),
    // and m is the number of coords.
    deltas: DMatrix<f64>,
}

impl Basis {
    fn eval(&self, r: f64) -> f64 {
        match self {
            Basis::PolyHarmonic(n) if n % 2 == 0 => {
                // Somewhat arbitrary but don't expect tiny nonzero values.
                if r < 1e-12 {
                    0.0
                } else {
                    r.powi(*n) * r.ln()
                }
            }
            Basis::PolyHarmonic(n) if *n == 1 => r,
            Basis::PolyHarmonic(n) => r.powi(*n),
            // Note: it might be slightly more efficient to pre-recip c, but
            // let's keep code clean for now.
            Basis::Gaussian(c) => (-(r / c).powi(2)).exp(),
            Basis::MultiQuadric(c) => r.hypot(*c),
            Basis::InverseMultiQuadric(c) => (r * r + c * c).powf(-0.5),
        }
    }
}

impl Scatter {
    pub fn eval(&self, coords: DVector<f64>) -> DVector<f64> {
        let n = self.centers.len();
        let basis = DVector::from_fn(self.deltas.ncols(), |row, _c| {
            if row < n {
                // component from basis functions
                self.basis.eval((&coords - &self.centers[row]).norm())
            } else if row == n {
                // constant component
                1.0
            } else {
                // linear component
                coords[row - n - 1]
            }
        });
        &self.deltas * basis
    }

    // The order for the polynomial part, meaning terms up to (order - 1) are included.
    // This usage is consistent with Wilna du Toit's masters thesis "Radial Basis
    // Function Interpolation"
    pub fn create(
        centers: Vec<DVector<f64>>,
        vals: Vec<DVector<f64>>,
        basis: Basis,
        order: usize,
    ) -> Scatter {
        let n = centers.len();
        // n x m matrix. There's probably a better way to do this, ah well.
        let mut vals = DMatrix::from_columns(&vals).transpose();
        let n_aug = match order {
            // Pure radial basis functions
            0 => n,
            // Constant term
            1 => n + 1,
            // Affine terms
            2 => n + 1 + centers[0].len(),
            _ => unimplemented!("don't yet support higher order polynomials"),
        };
        // Augment to n' x m matrix, where n' is the total number of basis functions.
        if n_aug > n {
            vals = vals.resize_vertically(n_aug, 0.0);
        }
        // We translate the system to center the mean at the origin so that when
        // the system is degenerate, the pseudoinverse below minimizes the linear
        // coefficients.
        let means: Vec<_> = if order == 2 {
            let n = centers.len();
            let n_recip = (n as f64).recip();
            (0..centers[0].len())
                .map(|i| centers.iter().map(|c| c[i]).sum::<f64>() * n_recip)
                .collect()
        } else {
            Vec::new()
        };
        let mat = DMatrix::from_fn(n_aug, n_aug, |r, c| {
            if r < n && c < n {
                basis.eval((&centers[r] - &centers[c]).norm())
            } else if r < n {
                if c == n {
                    1.0
                } else {
                    centers[r][c - n - 1] - means[c - n - 1]
                }
            } else if c < n {
                if r == n {
                    1.0
                } else {
                    centers[c][r - n - 1] - means[r - n - 1]
                }
            } else {
                0.0
            }
        });
        // inv is an n' x n' matrix.
        let inv = mat.try_inverse().expect("can't invert matrix");
        // Again, this transpose feels like I don't know what I'm doing.
        let mut deltas = (inv * vals).transpose();
        if order == 2 {
            let m = centers[0].len();
            for i in 0..deltas.nrows() {
                let offset: f64 = (0..m).map(|j| means[j] * deltas[(i, n + 1 + j)]).sum();
                deltas[(i, n)] -= offset;
            }
        }
        Scatter {
            basis,
            centers,
            deltas,
        }
    }
}


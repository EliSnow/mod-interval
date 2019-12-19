use futures::{
    future::{self, Either},
    stream::{self, Stream},
};

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

struct LinearSegment {
    m: f64,
    b: f64,
    zero_x: Option<f64>,
    duration: Duration,
}

impl LinearSegment {
    fn get_y(&self, x: f64) -> f64 {
        match (x == 0.0, self.zero_x) {
            (true, Some(y)) => y,
            _ => self.m * x + self.b,
        }
    }
}

// stored as per minute
pub struct PerX(u64);

impl PerX {
    pub fn minute(min: u64) -> Self {
        PerX(min)
    }

    pub fn second(sec: u64) -> Self {
        PerX(sec * 60)
    }

    fn as_per_second(&self) -> f64 {
        self.0 as f64 / 60.0
    }
}

struct ModIntervalStreamState {
    start_time: Instant,
    segment: LinearSegment,
    x_offset: Duration,
}

pub struct ModInterval {
    segments: VecDeque<LinearSegment>,
    duration: Duration,
}

impl ModInterval {
    pub fn new() -> Self {
        ModInterval {
            segments: VecDeque::new(),
            duration: Default::default(),
        }
    }

    pub fn append_segment(&mut self, start: PerX, duration: Duration, end: PerX) {
        let x1 = start.as_per_second();
        let x2 = end.as_per_second();
        let y2 = duration.as_secs_f64();
        let m = y2 / (x2 - x2);
        let b = x1;
        let zero_x = if x1 == 0.0 || x2 == 0.0 {
            Some(1000.0 / ((8.0 * m).sqrt() / (2.0 * m)))
        } else {
            None
        };

        self.duration += duration;
        let segment = LinearSegment {
            m,
            b,
            zero_x,
            duration,
        };
        self.segments.push_back(segment);
    }

    pub fn into_stream(mut self) -> impl Stream<Item = Instant> {
        let mut state = None;
        stream::unfold((), move |_| {
            let now = time::now();
            if state.is_none() {
                let segment = match self.segments.pop_front() {
                    Some(s) => s,
                    None => {
                        return Either::Left(future::ready(None));
                    }
                };
                let s = ModIntervalStreamState {
                    start_time: now,
                    segment,
                    x_offset: Default::default(),
                };
                state = Some(s);
            }
            let state = state.as_mut().unwrap();
            let mut x = now - state.start_time - state.x_offset;
            if x > state.segment.duration {
                let segment = match self.segments.pop_front() {
                    Some(s) => s,
                    None => {
                        return Either::Left(future::ready(None));
                    }
                };
                x -= state.segment.duration;
                state.x_offset += state.segment.duration;
                state.segment = segment;
            }

            let target_hits_per_second = state.segment.get_y(x.as_secs_f64());
            let y = Duration::from_secs_f64(1000.0 / target_hits_per_second);

            let right = async move {
                time::sleep(y).await;
                Some((now + y, ()))
            };
            Either::Right(right)
        })
    }
}

#[cfg(not(test))]
mod time {
    use super::*;
    use futures_timer::Delay;

    pub fn now() -> Instant {
        Instant::now()
    }

    pub async fn sleep(duration: Duration) {
        Delay::new(duration).await
    }
}

#[cfg(test)]
mod time {
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        pub static TIME_KEEPER: RefCell<Option<Instant>> = RefCell::new(None);
    }

    pub fn now() -> Instant {
        TIME_KEEPER.with(|t| {
            if t.borrow().is_none() {
                *t.borrow_mut() = Some(Instant::now());
            }
            t.borrow().clone().unwrap()
        })
    }

    pub async fn sleep(duration: Duration) {
        TIME_KEEPER.with(|t| {
            *t.borrow_mut() = t.borrow_mut().take().map(|i| i + duration);
        });
    }
}

// TODO: document public interface
// TODO: sleep should not extend past the duration of the ModInterval
// TODO: write tests
// does it work? does it work with multiple segments
// what happens when a `y` extends past the current segment (but not past the duration of ModInterval)?
#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

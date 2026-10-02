//! Timing and reach of the kaiju's close-range moves: the tail swipe and the stomp
//! (Godzilla and Zilla) and the bite (Zilla). Shared by the server (who gets hit)
//! and the client (the animation). Distances are blocks, angles radians, times
//! ticks since the move started.

/// Ground moves only reach what stands within this far above or below the kaiju's feet.
pub const GROUND_REACH_DY: f64 = 3.0;

pub(crate) fn smooth(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One tail segment, from the hips to the tip: blocks from the kaiju's centre and feet.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub width: f64,
    pub height: f64,
    pub forward: f64,
    pub up: f64,
    /// + is the kaiju's left.
    pub side: f64,
}

impl Segment {
    pub const fn new(width: f64, height: f64, forward: f64, up: f64, side: f64) -> Self {
        Self { width, height, forward, up, side }
    }
}

/// A tail swipe: the kaiju coils its tail to one side, then whips it round behind it to the other.
#[derive(Clone, Copy, Debug)]
pub struct Tail {
    pub segments: &'static [Segment],
    /// The tail swings about the point where it leaves the hips, this far behind the kaiju's centre.
    pub pivot_back: f64,
    pub windup: u32,
    pub sweep: u32,
    pub recover: u32,
    /// It winds up this far to one side of straight behind, and whips as far round to the other.
    pub arc: f64,
    /// It only swipes at things at least this far off its nose: beside or behind it.
    pub off_nose: f64,
    pub damage: f32,
    /// How hard what it hits is flung the way the tail goes (blocks per tick), and how high.
    pub knockback: f64,
    pub lift: f64,
}

impl Tail {
    /// How far from the pivot the tip reaches (the far edge of the last segment).
    pub fn reach(&self) -> f64 {
        let tip = self.segments[self.segments.len() - 1];
        -tip.forward - self.pivot_back + tip.width / 2.0
    }

    pub fn total(&self) -> u32 {
        self.windup + self.sweep + self.recover
    }

    /// How far the tail is swung round from straight behind (+ towards the kaiju's left),
    /// `t` ticks into a swipe sweeping towards `direction` (+1 its left, -1 its right).
    pub fn angle(&self, t: f64, direction: i32) -> f64 {
        let wound = -f64::from(direction) * self.arc;
        let (windup, sweep, recover) = (f64::from(self.windup), f64::from(self.sweep), f64::from(self.recover));
        if t <= 0.0 || t >= f64::from(self.total()) {
            return 0.0;
        }
        if t < windup {
            return wound * smooth(t / windup);
        }
        if t < windup + sweep {
            return wound * (std::f64::consts::PI * (t - windup) / sweep).cos();
        }
        -wound * (1.0 - smooth((t - windup - sweep) / recover))
    }

    /// Whether the tail is whipping round (and so can hit) on tick `t`.
    pub fn sweeping(&self, t: u32) -> bool {
        t > self.windup && t <= self.windup + self.sweep
    }

    /// Which way to sweep at a target at `bearing`: wind up on the far side and come round at it.
    pub fn direction_for(bearing: f64) -> i32 {
        if bearing >= 0.0 { 1 } else { -1 }
    }

    /// Whether to swipe at a target at `bearing` and `distance` from the pivot, whose body spans
    /// `bottom..top` above the kaiju's feet: beside or behind, well within reach, at the tail's height.
    pub fn worth_swiping(&self, bearing: f64, distance: f64, half_width: f64, bottom: f64, top: f64) -> bool {
        let (low, high) = self.span_at(distance);
        bearing.abs() <= std::f64::consts::PI - self.off_nose
            && distance - half_width <= self.reach() * 0.88
            && top >= low
            && bottom <= high
    }

    /// Whether the tail, as it moves on tick `t`, passes over a target at `bearing` and `distance`.
    #[allow(clippy::too_many_arguments)]
    pub fn hits(&self, t: u32, direction: i32, bearing: f64, distance: f64, half_width: f64, bottom: f64, top: f64) -> bool {
        if !self.sweeping(t) || distance - half_width > self.reach() {
            return false;
        }
        let (low, high) = self.span_at(distance);
        if top < low || bottom > high {
            return false;
        }
        // The tail curves off to one side a little: where it lies at this distance.
        let segment = self.segment_at(distance);
        let rest = segment.side.atan2((-segment.forward - self.pivot_back).max(1.0));
        let allowance = (segment.width / 2.0 + half_width).atan2(distance.max(1.0));
        let from = self.angle(f64::from(t) - 1.0, direction) + rest;
        let to = self.angle(f64::from(t), direction) + rest;
        bearing >= from.min(to) - allowance && bearing <= from.max(to) + allowance
    }

    /// How low and how high the tail reaches `distance` blocks from the pivot, above the kaiju's feet.
    pub fn span_at(&self, distance: f64) -> (f64, f64) {
        let mut low = f64::MAX;
        let mut high = f64::MIN;
        for s in self.segments {
            if (-s.forward - self.pivot_back - distance).abs() <= s.width / 2.0 {
                low = low.min(s.up);
                high = high.max(s.up + s.height);
            }
        }
        if low > high {
            let nearest = self.segment_at(distance);
            return (nearest.up, nearest.up + nearest.height);
        }
        (low, high)
    }

    /// The tail segment that lies `distance` blocks from the pivot (the nearest one).
    pub fn segment_at(&self, distance: f64) -> Segment {
        let mut best = self.segments[0];
        let mut best_gap = f64::MAX;
        for s in self.segments {
            let gap = (-s.forward - self.pivot_back - distance).abs();
            if gap < best_gap {
                best_gap = gap;
                best = *s;
            }
        }
        best
    }

    /// Where a point of the tail ({forward, side} from the centre, at rest) is with the tail swung by `angle`.
    pub fn swing_point(&self, forward: f64, side: f64, angle: f64) -> (f64, f64) {
        let back = -forward - self.pivot_back;
        let (sin, cos) = angle.sin_cos();
        let swung_back = back * cos - side * sin;
        let swung_side = back * sin + side * cos;
        (-swung_back - self.pivot_back, swung_side)
    }

    /// Bearing of {forward, side} seen from the pivot: 0 straight behind, + towards the left, +-pi ahead.
    pub fn bearing_from_pivot(&self, forward: f64, side: f64) -> f64 {
        side.atan2(-forward - self.pivot_back)
    }

    pub fn distance_from_pivot(&self, forward: f64, side: f64) -> f64 {
        (-forward - self.pivot_back).hypot(side)
    }
}

/// A stomp: the kaiju lifts the foot on the target's side and drives it down; a shockwave rolls out.
#[derive(Clone, Copy, Debug)]
pub struct Stomp {
    pub windup: u32,
    pub total: u32,
    /// The stomping foot lands this far ahead of the centre, and this far out to the side.
    pub foot_forward: f64,
    pub foot_side: f64,
    pub radius: f64,
    /// It stomps at things up to this far (near edge) from its centre.
    pub trigger: f64,
    pub wave_start: f64,
    /// Blocks per tick.
    pub wave_speed: f64,
    pub damage: f32,
    pub knockback: f64,
    pub lift: f64,
}

impl Stomp {
    pub fn worth_it(&self, distance: f64, half_width: f64) -> bool {
        distance - half_width <= self.trigger
    }

    /// Radius of the shockwave `ticks` after the foot lands.
    pub fn wave_radius(&self, ticks: u32) -> f64 {
        self.radius.min(self.wave_start + f64::from(ticks) * self.wave_speed)
    }

    /// Ticks after landing until the shockwave reaches its full radius.
    pub fn wave_ticks(&self) -> u32 {
        ((self.radius - self.wave_start) / self.wave_speed).ceil() as u32
    }

    pub fn reaches(&self, distance: f64, half_width: f64, since_landing: u32) -> bool {
        distance - half_width <= self.wave_radius(since_landing)
    }

    /// Full damage under the foot, falling to half at the edge of the shockwave.
    pub fn damage_at(&self, distance: f64) -> f32 {
        let t = (distance / self.radius).clamp(0.0, 1.0);
        (f64::from(self.damage) * (1.0 - 0.5 * t)) as f32
    }

    /// How high the stomping foot is lifted, 0 (planted) to 1, `t` ticks into the stomp.
    pub fn foot_lift(&self, t: f64) -> f64 {
        let windup = f64::from(self.windup);
        let drop = (windup / 5.0).max(3.0);
        let raised = windup - drop;
        if t <= 0.0 || t >= windup {
            return 0.0;
        }
        if t < raised {
            return smooth(t / raised);
        }
        let u = (t - raised) / drop;
        1.0 - u * u
    }
}

/// A bite: the kaiju rears back with its jaws open, lunges, and flings what it catches aside.
#[derive(Clone, Copy, Debug)]
pub struct Bite {
    pub windup: u32,
    pub snap: u32,
    pub recover: u32,
    pub min_forward: f64,
    pub max_forward: f64,
    pub half_width: f64,
    pub max_height: f64,
    pub damage: f32,
    pub fling: f64,
    pub fling_lift: f64,
}

impl Bite {
    pub fn total(&self) -> u32 {
        self.windup + self.snap + self.recover
    }

    pub fn snapping(&self, t: u32) -> bool {
        t > self.windup && t <= self.windup + self.snap
    }

    pub fn worth_it(&self, forward: f64, side: f64, half_width: f64, dy: f64) -> bool {
        forward - half_width <= self.max_forward - 1.0
            && forward + half_width >= self.min_forward + 0.5
            && side.abs() - half_width <= self.half_width - 0.5
            && dy >= -GROUND_REACH_DY
            && dy <= self.max_height - 1.0
    }

    pub fn hits(&self, t: u32, forward: f64, side: f64, half_width: f64, bottom: f64, top: f64) -> bool {
        self.snapping(t)
            && forward + half_width >= self.min_forward
            && forward - half_width <= self.max_forward
            && side.abs() - half_width <= self.half_width
            && bottom <= self.max_height
            && top >= -GROUND_REACH_DY
    }

    /// How far the head is drawn back (down to -1) or lunging (up to +1).
    pub fn lunge(&self, t: f64) -> f64 {
        let (windup, snap, recover) = (f64::from(self.windup), f64::from(self.snap), f64::from(self.recover));
        if t <= 0.0 || t >= f64::from(self.total()) {
            return 0.0;
        }
        if t < windup {
            return -smooth(t / windup);
        }
        if t < windup + snap {
            return -1.0 + 2.0 * smooth((t - windup) / snap);
        }
        1.0 - smooth((t - windup - snap) / recover)
    }

    /// How wide the jaws are open, 0 to 1.
    pub fn jaw(&self, t: f64) -> f64 {
        let (windup, snap) = (f64::from(self.windup), f64::from(self.snap));
        if t <= 0.0 || t >= f64::from(self.total()) {
            return 0.0;
        }
        if t < windup {
            return smooth(t / (windup * 0.7));
        }
        if t < windup + snap {
            return 1.0 - smooth((t - windup) / snap);
        }
        0.0
    }
}

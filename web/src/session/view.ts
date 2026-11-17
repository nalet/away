// How a machine's picture lies in the window. It is fitted to the window whole, larger
// or smaller than it is, and keeps its shape. From there two fingers zoom into it, and
// the part that shows follows the pointer.
//
// Each of these takes how large the fitted picture gets at most, in the window's pixels
// for one of its own: a desktop is enlarged as far as the window has room, and the
// windows of a program never are, which keep the size the machine draws them in.

export type Size = { width: number; height: number };

// Where the picture is looked at: how far it is zoomed into, where 1 is the whole of it
// fitted to the window, and which place of the picture is in the window's middle, in the
// picture's own pixels.
export type View = { zoom: number; x: number; y: number };

// The picture as it lies in the window, in the window's pixels: its corner, its size,
// and how many of the window's pixels one of its own is.
export type Placed = { left: number; top: number; width: number; height: number; scale: number };

// How far one zooms in at most: four times the whole, or until a pixel of the picture is
// two of the window's where that is more.
function most(picture: Size, window: Size, upTo: number): number {
  return Math.max(4, 2 / fitted(picture, window, upTo));
}

// How many of the window's pixels a pixel of the picture is when the whole of it fits.
export function fitted(picture: Size, window: Size, upTo = Infinity): number {
  if (picture.width <= 0 || picture.height <= 0 || window.width <= 0 || window.height <= 0) return 1;
  return Math.min(upTo, window.width / picture.width, window.height / picture.height);
}

// The whole picture, fitted to the window.
export function whole(picture: Size): View {
  return { zoom: 1, x: picture.width / 2, y: picture.height / 2 };
}

// Where the picture lies for a view. Along a side on which it is no larger than the
// window it sits in the middle; along one on which it is larger it never leaves a gap.
export function placed(picture: Size, window: Size, view: View, upTo = Infinity): Placed {
  const scale = fitted(picture, window, upTo) * view.zoom;
  const along = (size: number, room: number, middle: number) => {
    const long = size * scale;
    if (long <= room) return (room - long) / 2;
    return Math.min(0, Math.max(room - long, room / 2 - middle * scale));
  };
  return {
    left: along(picture.width, window.width, view.x),
    top: along(picture.height, window.height, view.y),
    width: picture.width * scale,
    height: picture.height * scale,
    scale,
  };
}

// The view as it really shows: zoomed no further than one may, and looking at a place
// that the window's middle can be at.
export function settled(picture: Size, window: Size, view: View, upTo = Infinity): View {
  const zoom = Math.min(most(picture, window, upTo), Math.max(1, view.zoom));
  const at = placed(picture, window, { ...view, zoom }, upTo);
  return { zoom, x: (window.width / 2 - at.left) / at.scale, y: (window.height / 2 - at.top) / at.scale };
}

// Zooms by a factor. The place of the picture that was under one point of the window is
// under another afterwards: two fingers that spread also carry the picture along.
export function zoomed(
  picture: Size,
  window: Size,
  view: View,
  by: number,
  from: { x: number; y: number },
  to: { x: number; y: number },
  upTo = Infinity,
): View {
  const before = placed(picture, window, view, upTo);
  const held = { x: (from.x - before.left) / before.scale, y: (from.y - before.top) / before.scale };
  const zoom = Math.min(most(picture, window, upTo), Math.max(1, view.zoom * by));
  const scale = fitted(picture, window, upTo) * zoom;
  return settled(picture, window, { zoom, x: held.x + (window.width / 2 - to.x) / scale, y: held.y + (window.height / 2 - to.y) / scale }, upTo);
}

// Moves the view so that a place of the picture shows, with room around it: the part
// that shows follows the pointer.
export function following(picture: Size, window: Size, view: View, place: { x: number; y: number }, upTo = Infinity): View {
  const at = placed(picture, window, view, upTo);
  const along = (where: number, corner: number, room: number, middle: number) => {
    const around = Math.min(64, room / 5);
    const shows = corner + where * at.scale;
    if (shows < around) return middle - (around - shows) / at.scale;
    if (shows > room - around) return middle + (shows - (room - around)) / at.scale;
    return middle;
  };
  return settled(
    picture,
    window,
    { zoom: view.zoom, x: along(place.x, at.left, window.width, view.x), y: along(place.y, at.top, window.height, view.y) },
    upTo,
  );
}

// The place nearest to one of the picture's that shows, with room around it: after two
// fingers zoomed or carried the picture, the pointer comes along into what shows.
export function showing(picture: Size, window: Size, view: View, place: { x: number; y: number }, upTo = Infinity): { x: number; y: number } {
  const at = placed(picture, window, view, upTo);
  const along = (where: number, corner: number, room: number, size: number) => {
    const around = Math.min(64, room / 5);
    const first = Math.max(0, (around - corner) / at.scale);
    const last = Math.min(size - 1, (room - around - corner) / at.scale);
    return first > last ? (first + last) / 2 : Math.max(first, Math.min(last, where));
  };
  return { x: along(place.x, at.left, window.width, picture.width), y: along(place.y, at.top, window.height, picture.height) };
}

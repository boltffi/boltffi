const success = value => ({ value });
const failure = error => ({ error });

export function capture(call, receiver, args) {
  try { return success(Reflect.apply(call, receiver, args)); }
  catch (error) { return failure(error); }
}
export function captureAsync(promise) {
  return promise.then(success, failure);
}

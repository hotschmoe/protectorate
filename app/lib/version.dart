// The build's version, set by the Makefile (PROTECTORATE_VERSION=$(VERSION));
// "dev" for a plain flutter run or test.
const appVersion = String.fromEnvironment(
  'PROTECTORATE_VERSION',
  defaultValue: 'dev',
);

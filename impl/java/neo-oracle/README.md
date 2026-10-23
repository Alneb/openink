# neo-oracle — run the device's own libneo_pen.so as a reference

A host-built dex run with `app_process` on the Boox (`/system/lib64/libneo_pen.so`, the same binary as
`firmware/onyx/pen/libneo_pen.so`). It feeds recorded pen samples to libneo through its JNI entry points and prints
the ink libneo returns, so openink-stroke can be checked against the original point by point
(docs/SF-INSTANT-INK.md, docs/STROKE.md).

The classes under `com/onyx/android/sdk/pen/` are **our own stubs**: they only carry the class, field and method
names libneo's JNI code looks up (read from its strings and from knote2's public field names); no Onyx code is copied.
The library itself stays on the device and is never put in this repo.

    bash impl/java/neo-oracle/run.sh PEN_TYPE SAMPLES.txt [field=value ...]   # prints the ink per call

// Stub library implementing the extern symbols declared in advanced_types.adesh
// (all the fixed-width integer/float/pointer type mappings).
#include <stdint.h>

int8_t   test_i8(int8_t v)      { return v; }
int16_t  test_i16(int16_t v)    { return v; }
int32_t  test_i32(int32_t v)    { return v; }
int64_t  test_i64(int64_t v)    { return v; }
__int128 test_i128(__int128 v)  { return v; }

uint8_t  test_u8(uint8_t v)     { return v; }
uint16_t test_u16(uint16_t v)   { return v; }
uint32_t test_u32(uint32_t v)   { return v; }
uint64_t test_u64(uint64_t v)   { return v; }
unsigned __int128 test_u128(unsigned __int128 v) { return v; }

float   test_f32(float v)       { return v; }
double  test_f64(double v)      { return v; }

int32_t* test_ptr(int32_t* p)              { return p; }
void*   test_void_ptr(void* p)             { return p; }

#ifndef WINGUI_H
#define WINGUI_H

#include <stdint.h>

// Window management
void* create_window(const char* title, int32_t width, int32_t height);
void show_window(void* hwnd);
int32_t process_messages(void* hwnd);
void run_message_loop(void* hwnd);  // Blocking message loop
void close_window(void* hwnd);

// Drawing functions
void clear_window(void* hwnd, uint32_t color);
void draw_circle(void* hwnd, int32_t x, int32_t y, int32_t radius, uint32_t color);
void draw_rectangle(void* hwnd, int32_t x, int32_t y, int32_t width, int32_t height, uint32_t color);
void draw_line(void* hwnd, int32_t x1, int32_t y1, int32_t x2, int32_t y2, uint32_t color);
void draw_ellipse(void* hwnd, int32_t x, int32_t y, int32_t width, int32_t height, uint32_t color);
void draw_pixel(void* hwnd, int32_t x, int32_t y, uint32_t color);

// Utility functions
void refresh_window(void* hwnd);
void set_window_title(void* hwnd, const char* title);

// Color helpers (RGB format: 0xRRGGBB)
#define RGB_RED     0xFF0000
#define RGB_GREEN   0x00FF00
#define RGB_BLUE    0x0000FF
#define RGB_YELLOW  0xFFFF00
#define RGB_CYAN    0x00FFFF
#define RGB_MAGENTA 0xFF00FF
#define RGB_WHITE   0xFFFFFF
#define RGB_BLACK   0x000000

#endif // WINGUI_H

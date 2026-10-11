#include "data_activity_led.h"
#include "dev_log.h"
#include <Arduino.h>
#include <driver/gpio.h>
#include <esp_timer.h>

static DataActivityLed led;
static esp_timer_handle_t timer = nullptr;
static portMUX_TYPE mutex = portMUX_INITIALIZER_UNLOCKED;

static void tick(void *) {
    portENTER_CRITICAL(&mutex);
    if (!led.tick(millis())) esp_timer_stop(timer);
    gpio_set_level(GPIO_NUM_3, led.on ? 0 : 1);
    portEXIT_CRITICAL(&mutex);
}

void dataLedBegin() {
    pinMode(3, OUTPUT);
    digitalWrite(3, HIGH);
    gpio_sleep_sel_dis(GPIO_NUM_3);
    esp_timer_create_args_t args = {};
    args.callback = tick;
    args.name = "data-led";
    args.skip_unhandled_events = true;
    if (esp_timer_create(&args, &timer) != ESP_OK)
        DevLog.println("[led] timer allocation failed; indicator disabled");
}

void dataLedActivity() {
    if (!timer) return;
    portENTER_CRITICAL(&mutex);
    if (led.activity(millis())) {
        if (esp_timer_start_periodic(timer, 80000) != ESP_OK) led = {};
        gpio_set_level(GPIO_NUM_3, led.on ? 0 : 1);
    }
    portEXIT_CRITICAL(&mutex);
}

void dataLedStop() {
    if (!timer) return; // Thin timer wakes never initialize the indicator.
    portENTER_CRITICAL(&mutex);
    esp_timer_stop(timer);
    led = {};
    gpio_set_level(GPIO_NUM_3, 1);
    portEXIT_CRITICAL(&mutex);
}

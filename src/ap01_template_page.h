#pragma once
#include <stdbool.h>
#include <stdint.h>

// Native page focus is independent from the platform's single active context.
// activate must finish the existing local atomic activation (including context
// persistence) before returning true. Failure/unknown must leave focus alone.
typedef bool (*Ap01TemplateActivate)(void *user, unsigned index);
typedef struct {
    bool focused;
    uint8_t index;
} Ap01TemplatePage;
typedef enum {
    AP01_TEMPLATE_REJECTED = -1,
    AP01_TEMPLATE_CHANGED = 0,
    AP01_TEMPLATE_EXIT_PREVIOUS = 1,
    AP01_TEMPLATE_EXIT_NEXT = 2
} Ap01TemplateMove;

static inline bool ap01TemplateEnter(Ap01TemplatePage *page, unsigned count,
                                    unsigned active, int direction,
                                    Ap01TemplateActivate activate, void *user) {
    if (!page || !count || count > 8 || active >= count || !direction || !activate) return false;
    unsigned next = direction > 0 ? 0 : count - 1;
    if (next != active && !activate(user, next)) return false;
    page->index = (uint8_t)next;
    page->focused = true;
    return true;
}
static inline Ap01TemplateMove ap01TemplateRotate(Ap01TemplatePage *page,
    unsigned count, int direction, Ap01TemplateActivate activate, void *user) {
    if (!page || !page->focused || !count || count > 8 || page->index >= count ||
        !direction || !activate) return AP01_TEMPLATE_REJECTED;
    if ((direction < 0 && page->index == 0) || (direction > 0 && page->index + 1 == count)) {
        page->focused = false;
        return direction < 0 ? AP01_TEMPLATE_EXIT_PREVIOUS : AP01_TEMPLATE_EXIT_NEXT;
    }
    unsigned next = direction > 0 ? page->index + 1 : page->index - 1;
    if (!activate(user, next)) return AP01_TEMPLATE_REJECTED;
    page->index = (uint8_t)next;
    return AP01_TEMPLATE_CHANGED;
}

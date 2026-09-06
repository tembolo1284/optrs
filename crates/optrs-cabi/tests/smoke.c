/* crates/optrs-cabi/tests/smoke.c
 *
 * Link against the cdylib and exercise the ABI the way a binding would.
 * Build:
 *   cargo build -p optrs-cabi --release
 *   cc -Iinclude tests/smoke.c -Ltarget/release -loptrs -o smoke
 */

#include <assert.h>
#include <math.h>
#include <stdio.h>
#include "optrs.h"

static void check(opt_status_t st, const char *what) {
    if (st != OPT_STATUS_T_OK) {
        const char *msg = opt_last_error_message();
        fprintf(stderr, "%s failed (%d): %s\n", what, (int)st, msg ? msg : "(none)");
        assert(0);
    }
}

int main(void) {
    printf("optrs %s, abi %u\n", opt_version(), opt_abi_version());
    assert(opt_sizeof_option() == sizeof(opt_option_t));
    assert(opt_sizeof_result() == sizeof(opt_result_t));

    opt_pricer_t *p = opt_pricer_new();
    assert(p != NULL);

    opt_option_t opt;
    check(opt_option_init(&opt), "option_init");
    opt.kind = OPT_KIND_T_CALL;
    opt.style = OPT_STYLE_T_EUROPEAN;
    opt.spot = 100.0; opt.strike = 100.0;
    opt.rate = 0.05;  opt.div_yield = 0.02;
    opt.vol = 0.25;   opt.time = 1.0;

    opt_result_t res;
    check(opt_result_init(&res), "result_init");
    check(opt_price(p, 0 /* analytic */, &opt, &res), "price analytic");
    printf("analytic call = %.10f\n", res.price);

    /* COS must match the closed form to near machine precision. */
    opt_result_t cos_res;
    check(opt_result_init(&cos_res), "result_init");
    check(opt_price(p, 1 /* cos */, &opt, &cos_res), "price cos");
    assert(fabs(cos_res.price - res.price) < 1e-9);

    /* Unsupported combination must fail cleanly, not crash. */
    opt.style = OPT_STYLE_T_AMERICAN;
    opt.kind = OPT_KIND_T_PUT;
    int supported = 1;
    check(opt_engine_supports(0, &opt, &supported), "supports");
    assert(supported == 0);
    assert(opt_price(p, 0, &opt, &res) == OPT_STATUS_T_UNSUPPORTED);
    printf("expected error: %s\n", opt_last_error_message());

    /* American put via the lattice, with greeks. */
    check(opt_set_tree_steps(p, 1001), "set steps");
    check(opt_greeks(p, 3 /* tree-lr */, &opt, &res), "greeks");
    printf("american put = %.6f delta = %.6f gamma = %.6f\n",
           res.price, res.delta, res.gamma);
    assert(res.delta < 0.0);   /* put delta is negative */
    assert(res.gamma > 0.0);

    /* Bermudan with quarterly dates. */
    double dates[4] = {0.25, 0.50, 0.75, 1.00};
    opt.style = OPT_STYLE_T_BERMUDAN;
    opt.dates = dates;
    opt.n_dates = 4;
    opt_result_t berm;
    check(opt_result_init(&berm), "result_init");
    check(opt_price(p, 3, &opt, &berm), "price bermudan");
    assert(berm.price <= res.price + 1e-9);  /* bounded above by american */

    /* Null-pointer handling must be an error, not a segfault. */
    assert(opt_price(p, 0, NULL, &res) == OPT_STATUS_T_NULL_POINTER);

    opt_pricer_free(p);
    opt_pricer_free(NULL);  /* must be safe */
    printf("smoke test passed\n");
    return 0;
}

#[doc(hidden)]
#[macro_export]
macro_rules! for_each_smoketest_template {
    ($callback:ident) => {
        $callback! {
            test_template_angular_ts => "angular-ts",
            test_template_astro_ts => "astro-ts",
            test_template_basic_cpp => "basic-cpp",
            test_template_basic_cs => "basic-cs",
            test_template_basic_rs => "basic-rs",
            test_template_basic_ts => "basic-ts",
            test_template_browser_ts => "browser-ts",
            test_template_bun_ts => "bun-ts",
            test_template_chat_console_cs => "chat-console-cs",
            test_template_chat_console_rs => "chat-console-rs",
            test_template_chat_react_ts => "chat-react-ts",
            test_template_deno_ts => "deno-ts",
            test_template_hangman_react_ts => "hangman-react-ts",
            test_template_llm_chat_ts => "llm-chat-ts",
            test_template_money_exchange_react_ts => "money-exchange-react-ts",
            test_template_nextjs_ts => "nextjs-ts",
            test_template_nodejs_ts => "nodejs-ts",
            test_template_nuxt_ts => "nuxt-ts",
            test_template_react_ts => "react-ts",
            test_template_remix_ts => "remix-ts",
            test_template_solid_ts => "solid-ts",
            test_template_svelte_ts => "svelte-ts",
            test_template_tanstack_ts => "tanstack-ts",
            test_template_vue_ts => "vue-ts",
        }
    };
}
